//! Workspace performance measurements (spatial spec §14, §16.8). They print
//! timings rather than assert exact numbers. Run with
//! `cargo test --release -p seshat-application --test workspace_perf -- --ignored --nocapture`.
//! Results and targets: docs/review/performance.md.

use std::time::{Duration, Instant};

use seshat_application::ports::ListingError;
use seshat_application::workspace::{Effect, WorkspaceService};
use seshat_domain::entities::{EntryKind, FileEntry, Location};
use seshat_domain::workspace::{PanelId, PanelState, Workspace};

fn loc(path: &str) -> Location {
    Location::Path(path.into())
}

/// A tree with `fan` folders under the root, `branches / fan` under each,
/// and a chain of `tail` more folders below those: `branches` branches.
fn build(branches: usize, fan: usize, tail: usize) -> Workspace {
    let mut ws = Workspace::new(loc("/w"), PanelState::default());
    let root = ws.active_panel();
    for i in 0..fan {
        for j in 0..branches / fan {
            ws.activate_panel(root).unwrap();
            let mut at = ws
                .open_child(root, loc(&format!("/w/a{i}")), None)
                .unwrap()
                .panel;
            at = ws
                .open_child(at, loc(&format!("/w/a{i}/b{j}")), None)
                .unwrap()
                .panel;
            for k in 0..tail {
                let parent = ws.panel(at).unwrap().location().clone();
                at = ws
                    .open_child(at, parent.child(format!("t{k}").as_ref()), None)
                    .unwrap()
                    .panel;
            }
        }
    }
    ws
}

fn time<R>(label: &str, runs: u32, mut f: impl FnMut() -> R) -> Duration {
    let started = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(f());
    }
    let per = started.elapsed() / runs;
    println!("{label:<44} {per:>12.2?} per call");
    per
}

fn entries(folder: &Location, n: usize) -> Vec<FileEntry> {
    (0..n)
        .map(|i| {
            let name = format!("f{i}.txt");
            FileEntry {
                location: folder.child(name.as_ref()),
                display_name: name.clone(),
                name: name.into(),
                kind: EntryKind::File,
                size: Some(1),
                modified: None,
                hidden: false,
                content_type: None,
                trash_info: None,
                identity: None,
            }
        })
        .collect()
}

/// Answers the service's effects at once, like a fast filesystem.
fn settle(service: &mut WorkspaceService, size: usize) -> usize {
    let mut loads = 0;
    loop {
        let effects = service.take_effects();
        if effects.is_empty() {
            return loads;
        }
        for effect in effects {
            if let Effect::Load {
                panel,
                location,
                generation,
            } = effect
            {
                loads += 1;
                let result: Result<_, ListingError> = Ok(entries(&location, size));
                service.listing_loaded(panel, generation, result);
            }
        }
    }
}

fn report(scale: &str, branches: usize, fan: usize, tail: usize) {
    println!("\n== {scale}");
    let started = Instant::now();
    let ws = build(branches, fan, tail);
    println!(
        "{:<44} {:>12.2?} ({} panels, {} branches)",
        "build",
        started.elapsed(),
        ws.panel_count(),
        ws.branch_count()
    );
    let ids: Vec<PanelId> = ws.panels().map(|p| p.id()).collect();

    let mut w = ws.clone();
    time("Workspace::branches", 200, || w.branches().len());
    time("Workspace::cycle_branch", 1000, || w.cycle_branch(true));
    time("Workspace::move_left + move_right", 1000, || {
        w.move_left();
        w.move_right()
    });
    let mut i = 0;
    time("Workspace::activate_panel (any panel)", 1000, || {
        i = (i * 7919 + 13) % ids.len();
        w.activate_panel(ids[i]).unwrap()
    });
    time("Workspace::check_invariants", 20, || {
        w.check_invariants().is_ok()
    });

    let mut churn = ws.clone();
    let root = churn.roots()[0];
    time("close a branch, reopen it", 200, || {
        let tip = churn.active_branch().id();
        if churn.close_branch(tip).is_ok() {
            churn.activate_panel(root).unwrap();
            churn.open_child(root, loc("/w/churn"), None).unwrap();
        }
    });

    // The service: 1,000 entries per folder, selections in 50 panels.
    let mut service = WorkspaceService::new(ws.clone(), PanelState::default());
    settle(&mut service, 1000);
    let mut loads = 0;
    let switching = time("WorkspaceService::cycle_branch (+ effects)", 500, || {
        service.cycle_branch(true);
        loads += settle(&mut service, 1000);
    });
    println!(
        "{:<44} {:>12} over 500 switches (cache of 16 listings)",
        "  folder reads", loads
    );
    let _ = switching;
    let mut focus_loads = 0;
    time("WorkspaceService::toggle_focus_mode", 1000, || {
        service.toggle_focus_mode();
        focus_loads += settle(&mut service, 1000);
    });
    assert_eq!(focus_loads, 0, "Focus Mode never reads folders");
    for &panel in ids.iter().take(50) {
        if let Some(Location::Path(path)) = service.location(panel).cloned() {
            let items = (0..100)
                .map(|n| Location::Path(path.join(format!("f{n}.txt"))))
                .collect();
            service.set_selection(panel, items);
        }
    }
    time("WorkspaceService::selection (50 panels)", 200, || {
        service.selection().len()
    });
    time("WorkspaceService::targets", 200, || service.targets().len());
}

#[test]
#[ignore = "benchmark"]
fn workspace_scaling() {
    report("R2 scale: 50 branches", 50, 10, 3);
    report("4x: 200 branches", 200, 20, 3);
    report("20x: 1,000 branches", 1000, 50, 3);
}
