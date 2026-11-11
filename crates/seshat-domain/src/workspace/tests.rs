use super::*;
use crate::Direction;
use crate::entities::{DirectoryIdentity, Location};
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
    ws.path(branch)
        .iter()
        .map(|&p| location_of(ws, p))
        .collect()
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
    let p = ws
        .open_child(d.panel, loc("/home/u/Documents/Projects"), None)
        .unwrap();
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
    let homes =
        s.ws.panels()
            .filter(|p| *p.location() == loc("/home/u"))
            .count();
    assert_eq!(homes, 1);
    assert_eq!(
        s.ws.path(s.docs_branch)[0],
        s.ws.path(s.downloads_branch)[0]
    );
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

    assert!(s.ws.cycle_branch(Direction::Next));
    assert_eq!(s.ws.active_panel(), s.downloads);
    assert_eq!(
        s.ws.panel(s.downloads).unwrap().state.options.sort.key,
        SortKey::Modified
    );
    assert!(s.ws.cycle_branch(Direction::Next));
    assert_eq!(
        s.ws.active_panel(),
        s.projects,
        "the branch kept its cursor"
    );
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
    let again =
        s.ws.open_child(s.home, loc("/home/u/Downloads"), None)
            .unwrap();
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
    let reports =
        s.ws.open_child(s.documents, loc("/home/u/Documents/Reports"), None)
            .unwrap();
    assert!(reports.new_branch);
    let reports_branch = s.ws.active_branch().id();
    // From Downloads' branch, reopen Documents from Home: the Reports
    // branch was active more recently than the Projects branch.
    s.ws.activate_branch(s.downloads_branch).unwrap();
    s.ws.move_left();
    let opened =
        s.ws.open_child(s.home, loc("/home/u/Documents"), None)
            .unwrap();
    assert_eq!(s.ws.active_branch().id(), reports_branch);
    assert_eq!(opened.resolution, Resolution::ReactivateExistingBranch);
    // C1: the branch comes back at its remembered panel, below Documents.
    assert_eq!(opened.panel, s.documents);
    assert_eq!(s.ws.active_panel(), reports.panel);
    ok(&s.ws);
}

#[test]
fn opening_from_a_middle_panel_never_replaces_the_existing_child() {
    let mut s = scenario();
    s.ws.activate_panel(s.documents).unwrap();
    let other =
        s.ws.open_child(s.documents, loc("/home/u/Documents/Other"), None)
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
    let opened =
        s.ws.open_root(loc("/home/u/Documents"), PanelState::default());
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
    let p = ws
        .open_child(docs, loc("/home/u/Documents/P"), None)
        .unwrap()
        .panel;
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
    s.ws.cycle_branch(Direction::Next);
    s.ws.open_child(s.ws.active_panel(), loc("/home/u/x"), None)
        .unwrap();
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
    s.ws.cycle_branch(Direction::Next);
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
    let c = ws
        .open_child(r, loc("/run/media/usb/a"), None)
        .unwrap()
        .panel;
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
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
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
                    // Identities independent of names: exercises renames
                    // (same identity, new name) and replacements (same
                    // name, new identity).
                    let location = ws.panel(any).unwrap().location().child(name.as_ref());
                    let identity = (rng.next(3) > 0).then(|| DirectoryIdentity {
                        device: 1,
                        inode: rng.next(6) as u64,
                    });
                    ws.open_directory(any, location, identity, None).unwrap();
                }
                3 => {
                    ws.move_left();
                }
                4 => {
                    ws.move_right();
                }
                5 => {
                    ws.cycle_branch(if rng.next(2) == 0 {
                        Direction::Next
                    } else {
                        Direction::Previous
                    });
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

// ── Context-aware navigation (2-CONTEXT-AWARE-NAVIGATION.md) ────────────

fn id(inode: u64) -> Option<DirectoryIdentity> {
    Some(DirectoryIdentity { device: 1, inode })
}

#[test]
fn nav_16_2_1_opening_a_child_continues_the_active_branch() {
    let mut ws = home();
    let h = ws.active_panel();
    let branch = ws.active_branch().id();
    let d = ws
        .open_directory(h, loc("/home/u/Documents"), id(10), None)
        .unwrap();
    assert_eq!(d.resolution, Resolution::ContinueActiveBranch);
    assert_eq!((d.branch, d.active, d.created), (branch, d.panel, true));
    let p = ws
        .open_directory(d.panel, loc("/home/u/Documents/Projects"), id(11), None)
        .unwrap();
    assert_eq!(p.resolution, Resolution::ContinueActiveBranch);
    assert_eq!(ws.branch_count(), 1);
    ok(&ws);
}

#[test]
fn nav_16_2_2_a_different_child_of_an_ancestor_creates_a_new_branch() {
    let mut ws = home();
    let h = ws.active_panel();
    ws.open_child(h, loc("/home/u/Documents"), None).unwrap();
    ws.move_left();
    let dl = ws.open_child(h, loc("/home/u/Downloads"), None).unwrap();
    assert_eq!(dl.resolution, Resolution::CreateNewBranch);
    assert!(dl.new_branch);
    assert_eq!(ws.branch_count(), 2);
    ok(&ws);
}

#[test]
fn nav_16_2_3_reopening_an_existing_child_reactivates_its_branch_with_downstream_panels() {
    // Home → Documents → Projects, then Home → Downloads (active).
    let mut s = scenario();
    let before = s.ws.panel_count();
    s.ws.move_left(); // Downloads branch, at Home
    let opened =
        s.ws.open_child(s.home, loc("/home/u/Documents"), None)
            .unwrap();
    assert_eq!(opened.resolution, Resolution::ReactivateExistingBranch);
    assert_eq!(opened.branch, s.docs_branch);
    // C1: the Documents branch was left at Home (moved left before
    // Downloads was opened), above Documents, so it lands on Documents...
    assert_eq!(s.ws.active_panel(), s.documents);
    // ...with Projects still downstream, and nothing duplicated.
    assert_eq!(s.ws.panel(s.documents).unwrap().children(), [s.projects]);
    assert_eq!(s.ws.panel_count(), before);
    ok(&s.ws);
}

#[test]
fn nav_c1_reactivation_restores_the_remembered_panel_below_the_folder() {
    let mut s = scenario();
    // Leave the Documents branch at Projects, then go to Downloads.
    s.ws.activate_panel(s.projects).unwrap();
    s.ws.activate_branch(s.downloads_branch).unwrap();
    s.ws.move_left();
    let opened =
        s.ws.open_child(s.home, loc("/home/u/Documents"), None)
            .unwrap();
    assert_eq!(opened.resolution, Resolution::ReactivateExistingBranch);
    assert_eq!(opened.active, s.projects);
    assert_eq!(s.ws.active_panel(), s.projects);
}

#[test]
fn nav_16_2_1_the_next_panel_of_the_active_branch_is_reused() {
    let mut s = scenario();
    s.ws.activate_panel(s.documents).unwrap();
    s.ws.move_left(); // on the Documents branch, at Home
    let opened =
        s.ws.open_child(s.home, loc("/home/u/Documents"), None)
            .unwrap();
    assert_eq!(opened.resolution, Resolution::ContinueActiveBranch);
    assert_eq!(
        s.ws.active_panel(),
        s.documents,
        "lands on the opened folder"
    );
    assert_eq!(s.ws.panel(s.documents).unwrap().children(), [s.projects]);
}

#[test]
fn nav_16_2_5_repeated_opens_never_duplicate() {
    let mut s = scenario();
    let before = (s.ws.panel_count(), s.ws.branch_count());
    for _ in 0..20 {
        s.ws.activate_panel(s.home).unwrap();
        s.ws.open_child(s.home, loc("/home/u/Documents"), None)
            .unwrap();
        s.ws.activate_panel(s.home).unwrap();
        s.ws.open_child(s.home, loc("/home/u/Downloads"), None)
            .unwrap();
        s.ws.activate_panel(s.documents).unwrap();
        s.ws.open_child(s.documents, loc("/home/u/Documents/Projects"), None)
            .unwrap();
    }
    assert_eq!((s.ws.panel_count(), s.ws.branch_count()), before);
    ok(&s.ws);
}

#[test]
fn nav_c3_a_renamed_folder_is_followed_with_everything_below_it() {
    let mut ws = home();
    let h = ws.active_panel();
    let old = ws
        .open_directory(h, loc("/home/u/Old"), id(42), None)
        .unwrap()
        .panel;
    let below = ws
        .open_directory(old, loc("/home/u/Old/sub"), id(43), None)
        .unwrap()
        .panel;
    let state = ws.state_mut(below).unwrap();
    state.cursor = Some(loc("/home/u/Old/sub/a.txt"));
    state.selection.replace([loc("/home/u/Old/sub/a.txt")]);
    ws.activate_panel(h).unwrap();

    let opened = ws
        .open_directory(h, loc("/home/u/New"), id(42), None)
        .unwrap();
    assert_eq!(opened.panel, old, "the same panel, not a new one");
    assert_eq!(ws.panel(old).unwrap().location(), &loc("/home/u/New"));
    assert_eq!(ws.panel(below).unwrap().location(), &loc("/home/u/New/sub"));
    let state = &ws.panel(below).unwrap().state;
    assert_eq!(state.cursor, Some(loc("/home/u/New/sub/a.txt")));
    assert_eq!(state.selection.items(), [loc("/home/u/New/sub/a.txt")]);
    assert_eq!(opened.refreshed, [old, below]);
    assert_eq!(ws.panel_count(), 3);
    ok(&ws);
}

#[test]
fn nav_c3_a_replaced_folder_never_inherits_the_old_state() {
    let mut ws = home();
    let h = ws.active_panel();
    let docs = ws
        .open_directory(h, loc("/home/u/Docs"), id(7), None)
        .unwrap()
        .panel;
    let sub = ws
        .open_directory(docs, loc("/home/u/Docs/sub"), id(8), None)
        .unwrap()
        .panel;
    let state = ws.state_mut(docs).unwrap();
    state.cursor = Some(loc("/home/u/Docs/old.txt"));
    state.options.sort.key = SortKey::Size;
    ws.activate_panel(h).unwrap();

    // Deleted and recreated: same name, different inode.
    let opened = ws
        .open_directory(h, loc("/home/u/Docs"), id(99), None)
        .unwrap();
    assert_eq!(opened.panel, docs);
    let p = ws.panel(docs).unwrap();
    assert_eq!(
        p.state.cursor, None,
        "the old cursor belonged to the old folder"
    );
    assert_eq!(
        p.state.options.sort.key,
        SortKey::Size,
        "view settings stay"
    );
    assert_eq!(p.identity(), id(99));
    assert_eq!(
        ws.panel(sub).unwrap().unavailable(),
        Some(Unavailability::Missing)
    );
    assert_eq!(opened.refreshed, [docs, sub]);
    ok(&ws);
}

#[test]
fn nav_c3_an_unknown_identity_is_learned_not_treated_as_a_change() {
    let mut ws = home();
    let h = ws.active_panel();
    let docs = ws.open_child(h, loc("/home/u/Docs"), None).unwrap().panel;
    ws.state_mut(docs).unwrap().cursor = Some(loc("/home/u/Docs/x"));
    ws.activate_panel(h).unwrap();
    let opened = ws
        .open_directory(h, loc("/home/u/Docs"), id(5), None)
        .unwrap();
    assert!(opened.refreshed.is_empty());
    assert_eq!(ws.panel(docs).unwrap().identity(), id(5));
    assert_eq!(
        ws.panel(docs).unwrap().state.cursor,
        Some(loc("/home/u/Docs/x"))
    );
}

#[test]
fn nav_c5_sidebar_locations_reuse_an_existing_root() {
    let mut s = scenario();
    let first = s.ws.open_root(loc("/mnt/usb"), PanelState::default());
    assert_eq!(first.resolution, Resolution::CreateNewBranch);
    let usb_child =
        s.ws.open_child(first.panel, loc("/mnt/usb/photos"), None)
            .unwrap()
            .panel;
    s.ws.activate_branch(s.docs_branch).unwrap();
    let before = (s.ws.panel_count(), s.ws.branch_count(), s.ws.roots().len());

    let again = s.ws.open_root(loc("/mnt/usb"), PanelState::default());
    assert_eq!(again.resolution, Resolution::ReactivateExistingBranch);
    assert_eq!(again.panel, first.panel);
    assert_eq!(
        s.ws.active_panel(),
        usb_child,
        "back where the branch was left"
    );
    assert_eq!(
        (s.ws.panel_count(), s.ws.branch_count(), s.ws.roots().len()),
        before,
        "no duplicate root"
    );
    ok(&s.ws);
}

#[test]
fn nav_identities_survive_persistence() {
    let mut ws = home();
    let h = ws.active_panel();
    let docs = ws
        .open_directory(h, loc("/home/u/Docs"), id(77), None)
        .unwrap()
        .panel;
    let restored = Workspace::from_parts(ws.to_parts()).unwrap();
    assert_eq!(restored.panel(docs).unwrap().identity(), id(77));
}

// ── Backward branch pruning (3-BACKWARD-BRANCH-PRUNING.md) ──────────────

/// Home → Documents → Projects → Rust → Cargo, active on Cargo (spec 3 §4).
fn cargo_chain() -> (Workspace, Vec<PanelId>) {
    let mut ws = home();
    let mut chain = vec![ws.active_panel()];
    for name in ["Documents", "Projects", "Rust", "Cargo"] {
        let from = *chain.last().unwrap();
        let location = ws.panel(from).unwrap().location().child(name.as_ref());
        chain.push(ws.open_child(from, location, None).unwrap().panel);
    }
    ok(&ws);
    (ws, chain)
}

fn prune_active(ws: &mut Workspace, motion: PruneMotion) -> Result<PrunedSegment, PruneRefusal> {
    let (branch, panel) = (ws.active_branch().id(), ws.active_panel());
    let pruned = ws.prune(branch, panel, motion);
    ok(ws);
    pruned
}

type Structure = (
    Vec<PanelParts>,
    Vec<(BranchId, PanelId, PanelId)>,
    BranchId,
    PanelId,
);

/// Everything structural, without logical clocks or the jump list.
fn structure(ws: &Workspace) -> Structure {
    let parts = ws.to_parts();
    let branches = parts
        .branches
        .iter()
        .map(|b| (b.id, b.tip, b.cursor))
        .collect();
    (parts.panels, branches, parts.active, ws.active_panel())
}

#[test]
fn prune_db_removes_one_navigation_step() {
    let (mut ws, chain) = cargo_chain();
    let branch = ws.active_branch().id();
    let pruned = prune_active(&mut ws, PruneMotion::Step).unwrap();
    assert_eq!(pruned.removed(), vec![chain[4]]);
    assert_eq!(pruned.destination(), chain[3]);
    assert!(!pruned.ends_branch());
    assert_eq!(ws.active_panel(), chain[3], "Rust is active");
    assert_eq!(ws.active_branch().id(), branch, "branch identity is kept");
    assert_eq!(ws.active_branch().tip(), chain[3]);
    assert!(ws.panel(chain[4]).is_none());
    assert_eq!(
        path_names(&ws, branch),
        [
            "/home/u",
            "/home/u/Documents",
            "/home/u/Documents/Projects",
            "/home/u/Documents/Projects/Rust"
        ]
    );
}

#[test]
fn prune_db_b_without_shared_ancestors_goes_back_to_the_root() {
    let (mut ws, chain) = cargo_chain();
    let pruned = prune_active(&mut ws, PruneMotion::Segment).unwrap();
    assert_eq!(pruned.removed(), chain[1..].to_vec());
    assert_eq!(ws.active_panel(), chain[0]);
    assert_eq!(ws.panel_count(), 1);
    assert!(!pruned.ends_branch());
}

#[test]
fn prune_db_b_goes_back_to_the_nearest_shared_ancestor_and_ends_the_branch() {
    // Home → Documents ─┬─ Projects → Rust → Cargo   (active)
    //                   └─ Notes
    let (mut ws, chain) = cargo_chain();
    let cargo_branch = ws.active_branch().id();
    let notes = ws
        .open_child(chain[1], loc("/home/u/Documents/Notes"), None)
        .unwrap();
    let notes_before = ws.panel(notes.panel).unwrap().clone();
    ws.activate_branch(cargo_branch).unwrap();

    let pruned = prune_active(&mut ws, PruneMotion::Segment).unwrap();
    assert_eq!(pruned.destination(), chain[1], "Documents");
    assert_eq!(pruned.removed(), chain[2..].to_vec());
    assert!(pruned.ends_branch());
    assert!(ws.branch(cargo_branch).is_none());
    assert_eq!(ws.active_branch().id(), notes.branch, "Notes takes over");
    assert_eq!(ws.active_panel(), chain[1]);
    assert_eq!(
        ws.panel(notes.panel),
        Some(&notes_before),
        "Notes untouched"
    );
    assert_eq!(ws.branch(notes.branch).unwrap().tip(), notes.panel);
}

#[test]
fn prune_db_and_db_b_coincide_directly_below_a_shared_ancestor() {
    let s = scenario();
    assert_eq!(s.ws.active_panel(), s.downloads);
    let step =
        s.ws.resolve_prune(s.downloads_branch, s.downloads, PruneMotion::Step);
    let segment =
        s.ws.resolve_prune(s.downloads_branch, s.downloads, PruneMotion::Segment);
    assert_eq!(step, segment);
    assert_eq!(step.unwrap().destination, s.home);
}

#[test]
fn prune_spec_7_shared_ancestors_and_other_branches_are_preserved() {
    let mut s = scenario();
    s.ws.state_mut(s.downloads).unwrap().cursor = Some(loc("/home/u/Downloads/a.iso"));
    s.ws.activate_panel(s.projects).unwrap();
    let downloads_before = s.ws.panel(s.downloads).unwrap().clone();

    let pruned = prune_active(&mut s.ws, PruneMotion::Segment).unwrap();
    assert_eq!(pruned.removed(), vec![s.documents, s.projects]);
    assert!(
        s.ws.panel(s.home).is_some(),
        "Home is still used by Downloads"
    );
    assert_eq!(s.ws.panel(s.downloads), Some(&downloads_before));
    let downloads = s.ws.branch(s.downloads_branch).unwrap();
    assert_eq!(downloads.tip(), s.downloads, "branch identity is stable");
    assert_eq!(s.ws.active_branch().id(), s.downloads_branch);
    assert_eq!(s.ws.active_panel(), s.home);
}

#[test]
fn prune_keeps_the_destination_state() {
    let (mut ws, chain) = cargo_chain();
    let state = ws.state_mut(chain[3]).unwrap();
    state.cursor = Some(loc("/home/u/Documents/Projects/Rust/Cargo"));
    state.options.sort.key = SortKey::Size;
    state.options.query = "car".into();
    state.mode = ViewMode::Grid;
    state
        .selection
        .insert(loc("/home/u/Documents/Projects/Rust/notes.md"));
    let before = ws.panel(chain[3]).unwrap().state.clone();
    prune_active(&mut ws, PruneMotion::Step).unwrap();
    assert_eq!(ws.panel(chain[3]).unwrap().state, before);
}

#[test]
fn prune_refuses_when_panels_continue_beyond_the_active_panel() {
    let (mut ws, _) = cargo_chain();
    ws.move_left();
    let before = ws.clone();
    assert_eq!(
        prune_active(&mut ws, PruneMotion::Step),
        Err(PruneRefusal::PanelsBeyond)
    );
    assert_eq!(ws, before, "a refused prune changes nothing");
}

#[test]
fn prune_refuses_on_a_shared_panel() {
    let mut s = scenario();
    s.ws.activate_panel(s.home).unwrap();
    let before = s.ws.clone();
    assert_eq!(
        prune_active(&mut s.ws, PruneMotion::Segment),
        Err(PruneRefusal::Shared { branches: 2 })
    );
    assert_eq!(s.ws, before);
}

#[test]
fn prune_refuses_at_a_root_and_for_stale_requests() {
    let mut ws = home();
    let before = ws.clone();
    assert_eq!(
        prune_active(&mut ws, PruneMotion::Step),
        Err(PruneRefusal::NoDestination)
    );
    assert_eq!(ws, before);

    let (mut ws, chain) = cargo_chain();
    let branch = ws.active_branch().id();
    let before = ws.clone();
    assert_eq!(
        ws.prune(branch, chain[2], PruneMotion::Step),
        Err(PruneRefusal::Stale),
        "only the active panel can be pruned from"
    );
    assert_eq!(
        ws.prune(BranchId::from_raw(999), chain[4], PruneMotion::Step),
        Err(PruneRefusal::Stale)
    );
    assert_eq!(ws, before);
}

#[test]
fn prune_removes_pruned_panels_from_the_jump_list() {
    let (mut ws, _) = cargo_chain();
    let pruned = prune_active(&mut ws, PruneMotion::Segment).unwrap();
    let parts = ws.to_parts();
    for id in pruned.removed() {
        assert!(!parts.back.contains(&id), "{id} left in the back list");
        assert!(
            !parts.forward.contains(&id),
            "{id} left in the forward list"
        );
    }
}

#[test]
fn prune_never_changes_focus_mode_even_when_a_branch_ends() {
    let mut s = scenario();
    s.ws.set_retain_focus_on_branch_switch(false);
    s.ws.activate_panel(s.projects).unwrap();
    s.ws.set_focus_mode(true);
    let pruned = prune_active(&mut s.ws, PruneMotion::Segment).unwrap();
    assert!(pruned.ends_branch());
    assert!(s.ws.focus_mode(), "pruning is not a branch switch (B4)");
    s.ws.restore_pruned(&pruned).unwrap();
    assert!(s.ws.focus_mode());
}

#[test]
fn prune_undo_restores_the_workspace() {
    let (mut ws, chain) = cargo_chain();
    let state = ws.state_mut(chain[4]).unwrap();
    state.cursor = Some(loc("/home/u/Documents/Projects/Rust/Cargo/Cargo.toml"));
    state
        .selection
        .insert(loc("/home/u/Documents/Projects/Rust/Cargo/src"));
    let before = structure(&ws);
    let pruned = prune_active(&mut ws, PruneMotion::Segment).unwrap();
    assert_eq!(pruned.deselected(), 1);
    ws.restore_pruned(&pruned).unwrap();
    ok(&ws);
    assert_eq!(structure(&ws), before);
}

#[test]
fn prune_undo_restores_an_ended_branch_and_the_order_of_branches() {
    // Documents (pruned) sits before Downloads under Home: undo must put
    // it back first, so `[`/`]` order is unchanged.
    let mut s = scenario();
    s.ws.activate_panel(s.projects).unwrap();
    let order: Vec<BranchId> = s.ws.branches().iter().map(|b| b.id()).collect();
    let before = structure(&s.ws);
    let pruned = prune_active(&mut s.ws, PruneMotion::Segment).unwrap();
    assert!(pruned.ends_branch());
    s.ws.restore_pruned(&pruned).unwrap();
    ok(&s.ws);
    assert_eq!(structure(&s.ws), before, "Downloads' cursor came back too");
    let after: Vec<BranchId> = s.ws.branches().iter().map(|b| b.id()).collect();
    assert_eq!(after, order);
}

#[test]
fn prune_undo_is_refused_when_the_workspace_changed_there() {
    // The branch grew again from the destination.
    let (mut ws, chain) = cargo_chain();
    let pruned = prune_active(&mut ws, PruneMotion::Step).unwrap();
    ws.open_child(chain[3], loc("/home/u/Documents/Projects/Rust/Cargo"), None)
        .unwrap();
    let before = ws.clone();
    assert_eq!(ws.restore_pruned(&pruned), Err(RestoreRefusal::Changed));
    assert_eq!(ws, before);

    // The ended branch's folder was reopened under the shared ancestor.
    let mut s = scenario();
    s.ws.activate_panel(s.projects).unwrap();
    let pruned = prune_active(&mut s.ws, PruneMotion::Segment).unwrap();
    s.ws.open_child(s.home, loc("/home/u/Documents"), None)
        .unwrap();
    assert_eq!(s.ws.restore_pruned(&pruned), Err(RestoreRefusal::Changed));

    // The destination itself was pruned since.
    let (mut ws, _) = cargo_chain();
    let first = prune_active(&mut ws, PruneMotion::Step).unwrap();
    prune_active(&mut ws, PruneMotion::Step).unwrap();
    assert_eq!(
        ws.restore_pruned(&first),
        Err(RestoreRefusal::DestinationGone)
    );
}

#[test]
fn prune_results_persist_and_restore() {
    let mut s = scenario();
    s.ws.activate_panel(s.projects).unwrap();
    prune_active(&mut s.ws, PruneMotion::Step).unwrap();
    let restored = Workspace::from_parts(s.ws.to_parts()).unwrap();
    assert_eq!(structure(&restored), structure(&s.ws));
}

#[test]
fn invariants_hold_across_random_sequences_with_pruning_and_undo() {
    for seed in 0..40 {
        let mut rng = Lcg(seed);
        let mut ws = home();
        let mut undo: Vec<PrunedSegment> = Vec::new();
        for step in 0..400 {
            let ids: Vec<PanelId> = ws.panels().map(Panel::id).collect();
            let any = ids[rng.next(ids.len())];
            let name = format!("d{}", rng.next(4));
            match rng.next(9) {
                0..=2 => {
                    let from = ws.active_panel();
                    let location = ws.panel(from).unwrap().location().child(name.as_ref());
                    ws.open_child(from, location, None).unwrap();
                }
                3 => {
                    ws.move_left();
                }
                4 => {
                    ws.activate_panel(any).unwrap();
                }
                5 => {
                    let branches: Vec<BranchId> = ws.branches().iter().map(|b| b.id()).collect();
                    let _ = ws.close_branch(branches[rng.next(branches.len())]);
                }
                6 | 7 => {
                    let motion = if rng.next(2) == 0 {
                        PruneMotion::Step
                    } else {
                        PruneMotion::Segment
                    };
                    let before = ws.clone();
                    let (branch, panel) = (ws.active_branch().id(), ws.active_panel());
                    match ws.prune(branch, panel, motion) {
                        Ok(pruned) => undo.push(pruned),
                        Err(_) => assert_eq!(ws, before, "seed {seed}, step {step}"),
                    }
                }
                _ => {
                    if let Some(pruned) = undo.pop() {
                        let before = ws.clone();
                        if ws.restore_pruned(&pruned).is_err() {
                            assert_eq!(ws, before, "seed {seed}, step {step}");
                        }
                    }
                }
            }
            if let Err(err) = ws.check_invariants() {
                panic!("seed {seed}, step {step}: {err}");
            }
        }
    }
}

#[test]
fn branches_are_numbered_from_one_in_spatial_order() {
    let mut ws = home();
    let root = ws.active_panel();
    ws.open_child(root, loc("/home/u/a"), None).unwrap();
    ws.open_child(root, loc("/home/u/b"), None).unwrap();
    let order: Vec<BranchId> = ws.branches().iter().map(|b| b.id()).collect();
    assert_eq!(order.len(), 2);
    assert_eq!(ws.branch_number(1), Some(order[0]));
    assert_eq!(ws.branch_number(2), Some(order[1]));
    assert_eq!(ws.branch_number(0), None, "numbers start at 1");
    assert_eq!(ws.branch_number(3), None);
    ok(&ws);
}
