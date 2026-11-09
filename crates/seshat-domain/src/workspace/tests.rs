use super::*;
use crate::entities::Location;
use crate::view::{SortKey, ViewMode};

fn loc(s: &str) -> Location {
    Location::Path(s.into())
}

fn home() -> Workspace {
    Workspace::new(loc("/home/u"), PanelState::default())
}

fn location_of(ws: &Workspace, id: PanelId) -> String {
    match ws.panel(id).unwrap().location() {
        Location::Path(p) => p.display().to_string(),
        Location::Trash(p) => format!("trash:{}", p.display()),
    }
}

fn path_names(ws: &Workspace, branch: BranchId) -> Vec<String> {
    ws.path(branch).iter().map(|&p| location_of(ws, p)).collect()
}

fn ok(ws: &Workspace) {
    ws.check_invariants().unwrap();
}

/// Home → Documents → Projects, back to Home, then Downloads (spec §3.2).
struct Scenario {
    ws: Workspace,
    home: PanelId,
    documents: PanelId,
    projects: PanelId,
    downloads: PanelId,
    docs_branch: BranchId,
    downloads_branch: BranchId,
}

fn scenario() -> Scenario {
    let mut ws = home();
    let home = ws.active_panel();
    let documents = ws.open_child(home, loc("/home/u/Documents"), None).unwrap();
    let projects = ws
        .open_child(documents.panel, loc("/home/u/Documents/Projects"), None)
        .unwrap();
    let docs_branch = ws.active_branch().id();
    assert!(ws.move_left());
    assert!(ws.move_left());
    assert_eq!(ws.active_panel(), home);
    let downloads = ws.open_child(home, loc("/home/u/Downloads"), None).unwrap();
    ok(&ws);
    Scenario {
        downloads_branch: ws.active_branch().id(),
        ws,
        home,
        documents: documents.panel,
        projects: projects.panel,
        downloads: downloads.panel,
        docs_branch,
    }
}

// ── Spec §16.1 Branching ────────────────────────────────────────────────

#[test]
fn spec_16_1_opening_children_extends_the_branch() {
    let mut ws = home();
    let h = ws.active_panel();
    let d = ws.open_child(h, loc("/home/u/Documents"), None).unwrap();
    assert!(d.created && !d.new_branch);
    let p = ws.open_child(d.panel, loc("/home/u/Documents/Projects"), None).unwrap();
    assert!(p.created && !p.new_branch);
    assert_eq!(ws.branch_count(), 1);
    assert_eq!(ws.panel(p.panel).unwrap().parent(), Some(d.panel));
    ok(&ws);
}

#[test]
fn spec_16_1_returning_home_and_opening_downloads_creates_a_separate_branch() {
    let s = scenario();
    assert_eq!(s.ws.branch_count(), 2);
    assert_ne!(s.docs_branch, s.downloads_branch);
    assert_eq!(
        path_names(&s.ws, s.docs_branch),
        ["/home/u", "/home/u/Documents", "/home/u/Documents/Projects"]
    );
    assert_eq!(
        path_names(&s.ws, s.downloads_branch),
        ["/home/u", "/home/u/Downloads"]
    );
    // Downloads is a child of Home, not of Projects.
    assert_eq!(s.ws.panel(s.downloads).unwrap().parent(), Some(s.home));
    assert!(s.ws.panel(s.projects).unwrap().children().is_empty());
}

#[test]
fn spec_16_1_shared_ancestor_exists_once() {
    let s = scenario();
    let homes = s
        .ws
        .panels()
        .filter(|p| *p.location() == loc("/home/u"))
        .count();
    assert_eq!(homes, 1);
    assert_eq!(s.ws.path(s.docs_branch)[0], s.ws.path(s.downloads_branch)[0]);
    assert_eq!(s.ws.panel_count(), 4);
}

#[test]
fn spec_16_1_switching_branches_restores_their_position_and_state() {
    let mut s = scenario();
    s.ws.state_mut(s.projects).unwrap().cursor = Some(loc("/home/u/Documents/Projects/a.rs"));
    s.ws.state_mut(s.downloads).unwrap().options.sort.key = SortKey::Modified;

    assert!(s.ws.activate_branch(s.docs_branch).unwrap());
    // The branch was left at Home (we moved left before opening Downloads).
    assert_eq!(s.ws.active_panel(), s.home);
    assert!(s.ws.move_right());
    assert!(s.ws.move_right());
    assert_eq!(s.ws.active_panel(), s.projects);
    assert_eq!(
        s.ws.panel(s.projects).unwrap().state.cursor,
        Some(loc("/home/u/Documents/Projects/a.rs"))
    );

    assert!(s.ws.cycle_branch(true));
    assert_eq!(s.ws.active_panel(), s.downloads);
    assert_eq!(
        s.ws.panel(s.downloads).unwrap().state.options.sort.key,
        SortKey::Modified
    );
    assert!(s.ws.cycle_branch(true));
    assert_eq!(s.ws.active_panel(), s.projects, "the branch kept its cursor");
    ok(&s.ws);
}

#[test]
fn spec_16_1_closing_a_branch_keeps_shared_ancestors() {
    let mut s = scenario();
    let removed = s.ws.close_branch(s.downloads_branch).unwrap();
    assert_eq!(removed, vec![s.downloads]);
    assert!(s.ws.panel(s.home).is_some());
    assert_eq!(s.ws.active_branch().id(), s.docs_branch);
    ok(&s.ws);

    // Closing the other way round removes Documents and Projects only.
    let mut s = scenario();
    let removed = s.ws.close_branch(s.docs_branch).unwrap();
    assert_eq!(removed, vec![s.projects, s.documents]);
    assert!(s.ws.panel(s.home).is_some());
    assert!(s.ws.panel(s.downloads).is_some());
    ok(&s.ws);
}

#[test]
fn the_last_branch_cannot_be_closed() {
    let mut ws = home();
    let b = ws.active_branch().id();
    assert_eq!(ws.close_branch(b), Err(WorkspaceError::LastBranch));
}

#[test]
fn closing_an_inactive_branch_keeps_the_active_one() {
    let mut s = scenario();
    assert_eq!(s.ws.active_branch().id(), s.downloads_branch);
    s.ws.close_branch(s.docs_branch).unwrap();
    assert_eq!(s.ws.active_panel(), s.downloads);
}

// ── Spec §16.2 Navigation ───────────────────────────────────────────────

#[test]
fn spec_16_2_horizontal_movement_stays_within_the_branch() {
    let mut s = scenario();
    s.ws.activate_panel(s.projects).unwrap();
    assert!(!s.ws.move_right(), "no panel beyond the tip");
    assert!(s.ws.move_left());
    assert_eq!(s.ws.active_panel(), s.documents);
    assert!(s.ws.move_left());
    assert!(!s.ws.move_left(), "no panel before the root");
    assert_eq!(s.ws.active_panel(), s.home);
    // Moving right from Home follows this branch, not Downloads.
    assert!(s.ws.move_right());
    assert_eq!(s.ws.active_panel(), s.documents);
    assert_eq!(s.ws.panel_count(), 4, "movement never creates panels");
    assert_eq!(s.ws.branch_count(), 2);
}

#[test]
fn spec_16_2_opening_an_existing_child_activates_it() {
    let mut s = scenario();
    s.ws.activate_branch(s.docs_branch).unwrap(); // at Home
    let again = s.ws.open_child(s.home, loc("/home/u/Downloads"), None).unwrap();
    assert_eq!(again.panel, s.downloads);
    assert!(!again.created);
    assert_eq!(s.ws.active_branch().id(), s.downloads_branch);
    assert_eq!(s.ws.panel_count(), 4);
    ok(&s.ws);
}

#[test]
fn opening_a_child_shared_by_branches_picks_the_most_recent_one() {
    // Home → Documents → {Projects, Reports}
    let mut s = scenario();
    s.ws.activate_panel(s.documents).unwrap();
    let reports = s
        .ws
        .open_child(s.documents, loc("/home/u/Documents/Reports"), None)
        .unwrap();
    assert!(reports.new_branch);
    let reports_branch = s.ws.active_branch().id();
    // From Downloads' branch, reopen Documents from Home: the Reports
    // branch was active more recently than the Projects branch.
    s.ws.activate_branch(s.downloads_branch).unwrap();
    s.ws.move_left();
    s.ws.open_child(s.home, loc("/home/u/Documents"), None).unwrap();
    assert_eq!(s.ws.active_branch().id(), reports_branch);
    assert_eq!(s.ws.active_panel(), s.documents);
    ok(&s.ws);
}

#[test]
fn opening_from_a_middle_panel_never_replaces_the_existing_child() {
    let mut s = scenario();
    s.ws.activate_panel(s.documents).unwrap();
    let other = s
        .ws
        .open_child(s.documents, loc("/home/u/Documents/Other"), None)
        .unwrap();
    assert!(other.new_branch);
    assert_eq!(s.ws.panel(s.documents).unwrap().children().len(), 2);
    assert!(s.ws.panel(s.projects).is_some());
    assert_eq!(s.ws.branch_count(), 3);
    ok(&s.ws);
}

#[test]
fn new_children_inherit_presentation_but_not_position() {
    let mut ws = home();
    let h = ws.active_panel();
    let state = ws.state_mut(h).unwrap();
    state.options.show_hidden = true;
    state.options.query = "foo".into();
    state.mode = ViewMode::Grid;
    state.cursor = Some(loc("/home/u/x"));
    let child = ws.open_child(h, loc("/home/u/x"), None).unwrap().panel;
    let child = &ws.panel(child).unwrap().state;
    assert!(child.options.show_hidden);
    assert_eq!(child.mode, ViewMode::Grid);
    assert!(child.options.query.is_empty());
    assert_eq!(child.cursor, None);
}

#[test]
fn open_root_activates_a_panel_on_the_active_branch() {
    let mut s = scenario();
    s.ws.activate_panel(s.projects).unwrap();
    let opened = s.ws.open_root(loc("/home/u/Documents"), PanelState::default());
    assert_eq!(opened.panel, s.documents);
    assert!(!opened.created);
}

#[test]
fn open_root_elsewhere_starts_a_new_root_branch() {
    let mut s = scenario();
    let opened = s.ws.open_root(loc("/mnt/usb"), PanelState::default());
    assert!(opened.created && opened.new_branch);
    assert!(s.ws.panel(opened.panel).unwrap().is_root());
    assert_eq!(s.ws.roots().len(), 2);
    assert_eq!(s.ws.branch_count(), 3);
    ok(&s.ws);
}

#[test]
fn insert_parent_reroots_without_changing_branches() {
    let mut ws = Workspace::new(loc("/home/u/Documents"), PanelState::default());
    let docs = ws.active_panel();
    let p = ws.open_child(docs, loc("/home/u/Documents/P"), None).unwrap().panel;
    ws.move_left();
    let parent = ws.insert_parent(docs).unwrap();
    assert_eq!(location_of(&ws, parent), "/home/u");
    assert_eq!(ws.active_panel(), parent);
    assert_eq!(ws.roots(), [parent]);
    assert_eq!(ws.branch_count(), 1);
    assert_eq!(ws.active_path(), [parent, docs, p]);
    assert_eq!(ws.insert_parent(docs), Err(WorkspaceError::NotARoot));
    ok(&ws);

    let mut root = Workspace::new(loc("/"), PanelState::default());
    let r = root.active_panel();
    assert_eq!(root.insert_parent(r), Err(WorkspaceError::NoParent));
}

#[test]
fn back_and_forward_follow_panel_activations() {
    let mut s = scenario();
    // History: Home, Documents, Projects, Documents, Home, Downloads.
    assert!(s.ws.go_back());
    assert_eq!(s.ws.active_panel(), s.home);
    assert!(s.ws.go_back());
    assert_eq!(s.ws.active_panel(), s.documents);
    assert!(s.ws.go_forward());
    assert_eq!(s.ws.active_panel(), s.home);
    assert!(s.ws.go_forward());
    assert_eq!(s.ws.active_panel(), s.downloads);
    assert!(!s.ws.go_forward());
    assert_eq!(s.ws.panel_count(), 4, "jumps never create panels");
    ok(&s.ws);
}

#[test]
fn back_skips_closed_panels() {
    let mut s = scenario();
    s.ws.activate_panel(s.projects).unwrap();
    s.ws.activate_branch(s.downloads_branch).unwrap();
    s.ws.close_branch(s.docs_branch).unwrap();
    while s.ws.go_back() {
        assert!(s.ws.panel(s.ws.active_panel()).is_some());
    }
    ok(&s.ws);
}

// ── Spec §16.4 Focus Mode (model side) ──────────────────────────────────

#[test]
fn spec_16_4_focus_mode_changes_nothing_but_presentation() {
    let mut s = scenario();
    let before = (s.ws.to_parts(), s.ws.active_panel());
    assert!(s.ws.set_focus_mode(true));
    let mut after = s.ws.to_parts();
    after.focus_mode = false;
    assert_eq!((after, s.ws.active_panel()), before);
}

#[test]
fn spec_16_4_navigation_and_branch_switching_keep_focus_mode() {
    let mut s = scenario();
    s.ws.set_focus_mode(true);
    s.ws.move_left();
    s.ws.move_right();
    s.ws.cycle_branch(true);
    s.ws.open_child(s.ws.active_panel(), loc("/home/u/x"), None).unwrap();
    assert!(s.ws.focus_mode());
    ok(&s.ws);
}

#[test]
fn focus_mode_can_be_dropped_on_branch_switch_by_preference() {
    let mut s = scenario();
    s.ws.set_retain_focus_on_branch_switch(false);
    s.ws.set_focus_mode(true);
    s.ws.move_left(); // same branch
    assert!(s.ws.focus_mode());
    s.ws.cycle_branch(true);
    assert!(!s.ws.focus_mode());
}

#[test]
fn closing_the_active_branch_in_focus_mode_keeps_focus_mode() {
    let mut s = scenario();
    s.ws.set_focus_mode(true);
    s.ws.close_branch(s.downloads_branch).unwrap();
    assert!(s.ws.focus_mode());
    assert!(s.ws.panel(s.ws.active_panel()).is_some());
}

// ── Availability ────────────────────────────────────────────────────────

#[test]
fn unavailable_panels_and_their_descendants_are_kept() {
    let mut s = scenario();
    let affected = s.ws.mark_unavailable(s.documents, Unavailability::Missing);
    assert_eq!(affected, vec![s.documents, s.projects]);
    assert_eq!(s.ws.panel_count(), 4);
    assert_eq!(s.ws.nearest_available_ancestor(s.projects), Some(s.home));
    assert_eq!(s.ws.panel(s.downloads).unwrap().unavailable(), None);
    s.ws.mark_available(s.documents);
    assert_eq!(s.ws.panel(s.documents).unwrap().unavailable(), None);
}

#[test]
fn panels_within_finds_panels_on_a_device() {
    let mut ws = Workspace::new(loc("/run/media/usb"), PanelState::default());
    let r = ws.active_panel();
    let c = ws.open_child(r, loc("/run/media/usb/a"), None).unwrap().panel;
    ws.open_root(loc("/home/u"), PanelState::default());
    let mut within = ws.panels_within(std::path::Path::new("/run/media/usb"));
    within.sort();
    assert_eq!(within, vec![r, c]);
}

// ── Selection ───────────────────────────────────────────────────────────

#[test]
fn selection_toggles_replaces_and_prunes() {
    let mut sel = Selection::default();
    assert!(sel.toggle(loc("/a")));
    assert!(sel.toggle(loc("/b")));
    assert!(!sel.toggle(loc("/a")));
    assert_eq!(sel.items(), [loc("/b")]);
    sel.replace([loc("/c"), loc("/d"), loc("/c")]);
    assert_eq!(sel.items(), [loc("/c"), loc("/d")]);
    sel.set_anchor(Some(loc("/d")));
    assert_eq!(sel.retain(|l| *l != loc("/d")), 1);
    assert_eq!(sel.anchor(), None);
    sel.clear();
    assert!(sel.is_empty());
}

// ── Persistence round trip ──────────────────────────────────────────────

#[test]
fn parts_round_trip_and_reject_corruption() {
    let s = scenario();
    let parts = s.ws.to_parts();
    let restored = Workspace::from_parts(parts.clone()).unwrap();
    assert_eq!(restored.to_parts(), parts);
    assert_eq!(restored.active_panel(), s.ws.active_panel());

    let mut bad = parts.clone();
    bad.branches.pop();
    assert!(Workspace::from_parts(bad).is_err(), "leaf without a branch");

    let mut bad = parts.clone();
    bad.panels.retain(|p| p.id != s.projects);
    assert!(Workspace::from_parts(bad).is_err(), "dangling child");

    let mut bad = parts;
    bad.active = BranchId::from_raw(999);
    assert!(Workspace::from_parts(bad).is_err(), "unknown active branch");
}

#[test]
fn restored_workspaces_issue_fresh_ids() {
    let s = scenario();
    let mut restored = Workspace::from_parts(s.ws.to_parts()).unwrap();
    let new = restored
        .open_child(s.home, loc("/home/u/New"), None)
        .unwrap()
        .panel;
    assert!(s.ws.panel(new).is_none());
    ok(&restored);
}

// ── Randomised sequences ────────────────────────────────────────────────

/// Small deterministic generator, so failures reproduce exactly.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

#[test]
fn invariants_hold_across_random_operation_sequences() {
    for seed in 0..40 {
        let mut rng = Lcg(seed);
        let mut ws = home();
        for step in 0..400 {
            let ids: Vec<PanelId> = ws.panels().map(Panel::id).collect();
            let any = ids[rng.next(ids.len())];
            let name = format!("d{}", rng.next(4));
            match rng.next(12) {
                0 | 1 => {
                    let from = ws.active_panel();
                    let location = ws.panel(from).unwrap().location().child(name.as_ref());
                    ws.open_child(from, location, None).unwrap();
                }
                2 => {
                    let location = ws.panel(any).unwrap().location().child(name.as_ref());
                    ws.open_child(any, location, None).unwrap();
                }
                3 => {
                    ws.move_left();
                }
                4 => {
                    ws.move_right();
                }
                5 => {
                    ws.cycle_branch(rng.next(2) == 0);
                }
                6 => {
                    let branches: Vec<BranchId> = ws.branches().iter().map(|b| b.id()).collect();
                    let b = branches[rng.next(branches.len())];
                    let _ = ws.close_branch(b);
                }
                7 => {
                    ws.activate_panel(any).unwrap();
                }
                8 => {
                    ws.go_back();
                }
                9 => {
                    ws.go_forward();
                }
                10 => {
                    ws.set_focus_mode(rng.next(2) == 0);
                }
                _ => {
                    ws.open_root(loc(&format!("/r{}", rng.next(3))), PanelState::default());
                }
            }
            if let Err(err) = ws.check_invariants() {
                panic!("seed {seed}, step {step}: {err}");
            }
        }
    }
}
