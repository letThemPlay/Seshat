//! Workspace service tests. A fake shell answers `Load` effects from an
//! in-memory folder tree, so every reload is counted exactly.

use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use seshat_domain::Direction;
use seshat_domain::entities::{EntryKind, FileEntry, Location};
use seshat_domain::operation::OperationStatus;
use seshat_domain::search::SearchResult;
use seshat_domain::view::{SortKey, ViewMode};
use seshat_domain::workspace::{PanelId, PanelState, ScrollAnchor, Unavailability};

use super::*;
use crate::operations::OperationManager;
use crate::operations::memory_fs::MemoryFs;
use crate::ports::{DirectoryChange, ListingError};

fn loc(s: &str) -> Location {
    Location::Path(s.into())
}

fn entry(path: &str, dir: bool) -> FileEntry {
    let name = path.rsplit('/').next().unwrap();
    FileEntry {
        name: name.into(),
        display_name: name.into(),
        location: loc(path),
        kind: if dir {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        size: (!dir).then_some(1),
        modified: None,
        hidden: name.starts_with('.'),
        content_type: None,
        trash_info: None,
        identity: None,
    }
}

/// A fake filesystem for listings, plus a log of the I/O the service asked for.
struct Shell {
    service: WorkspaceService,
    tree: BTreeMap<String, Vec<(String, bool)>>,
    loads: Vec<Location>,
    watched: BTreeMap<PanelId, u64>,
}

impl Shell {
    fn new() -> Self {
        let mut tree = BTreeMap::new();
        tree.insert(
            "/h".into(),
            vec![
                ("Documents".into(), true),
                ("Downloads".into(), true),
                ("notes.txt".into(), false),
                (".hidden".into(), false),
            ],
        );
        tree.insert(
            "/h/Documents".into(),
            vec![
                ("Projects".into(), true),
                ("a.txt".into(), false),
                ("b.txt".into(), false),
            ],
        );
        tree.insert(
            "/h/Documents/Projects".into(),
            vec![("x.rs".into(), false), ("y.rs".into(), false)],
        );
        tree.insert("/h/Downloads".into(), vec![("big.iso".into(), false)]);
        let mut shell = Self {
            service: WorkspaceService::starting_at(loc("/h"), PanelState::default()),
            tree,
            loads: Vec::new(),
            watched: BTreeMap::new(),
        };
        shell.run();
        shell
    }

    /// Carries out pending effects until none are left.
    fn run(&mut self) {
        loop {
            let effects = self.service.take_effects();
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                match effect {
                    Effect::Load {
                        panel,
                        location,
                        generation,
                    } => {
                        self.loads.push(location.clone());
                        let result = self.list(&location);
                        self.service.listing_loaded(panel, generation, result);
                    }
                    Effect::Watch {
                        panel, generation, ..
                    } => {
                        self.watched.insert(panel, generation);
                    }
                    Effect::Unwatch { panel } => {
                        self.watched.remove(&panel);
                    }
                    Effect::Launch(_) | Effect::Persist => {}
                }
            }
        }
    }

    fn list(&self, location: &Location) -> Result<Vec<FileEntry>, ListingError> {
        let Location::Path(path) = location else {
            return Err(ListingError::NotFound);
        };
        let path = path.to_string_lossy();
        self.tree
            .get(path.as_ref())
            .map(|children| {
                children
                    .iter()
                    .map(|(name, dir)| entry(&format!("{path}/{name}"), *dir))
                    .collect()
            })
            .ok_or(ListingError::NotFound)
    }

    fn active(&self) -> PanelId {
        self.service.active_panel()
    }

    fn open(&mut self, name: &str) -> PanelId {
        let panel = self.active();
        let folder = self.service.location(panel).cloned().unwrap();
        let Location::Path(folder) = folder else {
            panic!()
        };
        let e = entry(&format!("{}/{name}", folder.display()), true);
        self.service.open_entry(panel, &e).unwrap();
        self.run();
        self.active()
    }

    fn loads_since(&mut self) -> usize {
        std::mem::take(&mut self.loads).len()
    }

    fn shown(&self, panel: PanelId) -> Vec<String> {
        self.service
            .listing(panel)
            .unwrap()
            .shown()
            .iter()
            .map(|e| e.display_name.clone())
            .collect()
    }
}

/// Home → Documents → Projects; back to Home; Downloads.
fn scenario() -> (Shell, [PanelId; 4]) {
    let mut s = Shell::new();
    let home = s.active();
    let documents = s.open("Documents");
    let projects = s.open("Projects");
    s.service.move_left();
    s.service.move_left();
    s.run();
    let downloads = s.open("Downloads");
    (s, [home, documents, projects, downloads])
}

// Spec §16.1 / §19

#[test]
fn spec_19_branches_coexist_and_navigate_independently() {
    let (mut s, [home, documents, projects, downloads]) = scenario();
    let ws = s.service.workspace();
    assert_eq!(ws.branch_count(), 2);
    assert_eq!(ws.panel(downloads).unwrap().parent(), Some(home));
    assert_eq!(ws.panel(projects).unwrap().parent(), Some(documents));

    // Switch to the Projects branch and keep navigating it.
    s.service.cycle_branch(Direction::Previous);
    s.run();
    s.service.move_right();
    s.service.move_right();
    assert_eq!(s.active(), projects);
    s.service.move_cursor(projects, 1);
    assert_eq!(
        s.service.state(projects).unwrap().cursor,
        Some(loc("/h/Documents/Projects/x.rs"))
    );
    // Downloads is untouched.
    assert_eq!(s.service.state(downloads).unwrap().cursor, None);
}

// Spec §16.3 Panel history

#[test]
fn spec_16_3_panel_state_is_restored_when_returning() {
    let (mut s, [_, documents, _, downloads]) = scenario();
    s.service.activate_panel(documents).unwrap();
    s.run();
    s.service.move_cursor(documents, 2);
    s.service.set_scroll(
        documents,
        Some(ScrollAnchor {
            item: loc("/h/Documents/a.txt"),
            offset: 12,
        }),
    );
    s.service.set_sort_key(documents, SortKey::Size);
    s.service.set_view_mode(documents, ViewMode::Grid);
    s.service.set_query(documents, ".txt");
    s.service
        .set_selection(documents, vec![loc("/h/Documents/b.txt")]);

    s.service.activate_panel(downloads).unwrap();
    s.service.activate_panel(documents).unwrap();
    s.run();

    let state = s.service.state(documents).unwrap();
    assert_eq!(state.cursor, Some(loc("/h/Documents/b.txt")));
    assert_eq!(state.scroll.as_ref().unwrap().offset, 12);
    assert_eq!(state.options.sort.key, SortKey::Size);
    assert_eq!(state.mode, ViewMode::Grid);
    assert_eq!(state.options.query, ".txt");
    assert_eq!(state.selection.items(), [loc("/h/Documents/b.txt")]);
    assert_eq!(s.shown(documents), ["a.txt", "b.txt"]);
}

#[test]
fn spec_16_3_external_changes_keep_valid_state_and_drop_stale_selections() {
    let (mut s, [_, documents, _, _]) = scenario();
    s.service.activate_panel(documents).unwrap();
    s.run();
    s.service.cursor_to_last(documents); // b.txt
    s.service.set_selection(
        documents,
        vec![loc("/h/Documents/a.txt"), loc("/h/Documents/b.txt")],
    );
    // b.txt is deleted elsewhere.
    s.tree
        .get_mut("/h/Documents")
        .unwrap()
        .retain(|(n, _)| n != "b.txt");
    let generation = s.watched[&documents];
    assert!(
        s.service
            .directory_changed(documents, generation, DirectoryChange::Modified)
    );
    s.service.reload_if_current(documents, generation);
    s.run();
    let state = s.service.state(documents).unwrap();
    assert_eq!(state.selection.items(), [loc("/h/Documents/a.txt")]);
    assert_eq!(
        state.cursor,
        Some(loc("/h/Documents/a.txt")),
        "the nearest remaining item"
    );
}

#[test]
fn search_filters_belong_to_their_panel() {
    let (mut s, [home, documents, _, _]) = scenario();
    s.service.set_query(documents, "a.");
    assert_eq!(s.shown(documents), ["a.txt"]);
    assert_eq!(s.shown(home), ["Documents", "Downloads", "notes.txt"]);
    s.service.set_query(documents, "");
    assert_eq!(s.shown(documents), ["Projects", "a.txt", "b.txt"]);
    s.service.set_query(documents, "zzz");
    assert_eq!(
        s.service.listing(documents).unwrap().content(),
        &Content::NoResults
    );
}

#[test]
fn filtered_out_items_never_stay_selected() {
    let (mut s, [_, documents, _, _]) = scenario();
    s.service.set_selection(
        documents,
        vec![loc("/h/Documents/Projects"), loc("/h/Documents/a.txt")],
    );
    s.service.set_query(documents, "a.txt");
    assert_eq!(
        s.service.state(documents).unwrap().selection.items(),
        [loc("/h/Documents/a.txt")],
        "an operation must not act on what the filter hides"
    );
    // Without a selection, the target is the visible cursor item.
    s.service.set_query(documents, "b.");
    s.service.activate_panel(documents).unwrap();
    s.service.cursor_to_first(documents);
    assert_eq!(s.service.targets()[0].items, [loc("/h/Documents/b.txt")]);
}

#[test]
fn open_panels_lend_their_listing_to_completion_without_reloading() {
    let (mut s, [home, ..]) = scenario();
    s.loads_since();
    let listing = s
        .service
        .listing_for(&loc("/h"))
        .expect("Home is open and loaded");
    assert_eq!(listing.entries().len(), 4);
    assert!(s.service.listing_for(&loc("/elsewhere")).is_none());
    s.run();
    assert_eq!(s.loads_since(), 0);
    let _ = home;
}

#[test]
fn settings_change_one_panel_and_become_defaults() {
    let (mut s, [home, documents, _, _]) = scenario();
    s.service.toggle_hidden(home);
    assert!(s.shown(home).contains(&".hidden".to_owned()));
    assert!(!s.service.state(documents).unwrap().options.show_hidden);
    assert!(s.service.defaults().options.show_hidden);
    s.service.open_location(loc("/h/Downloads/../Downloads"));
    s.run();
    assert!(s.service.state(s.active()).unwrap().options.show_hidden);
}

// Spec §16.4 / §16.8: no unnecessary reloads

#[test]
fn spec_16_4_focus_mode_never_reloads() {
    let (mut s, _) = scenario();
    s.loads_since();
    s.service.toggle_focus_mode();
    s.service.move_left();
    s.service.cycle_branch(Direction::Next);
    s.service.toggle_focus_mode();
    s.run();
    assert_eq!(s.loads_since(), 0);
}

#[test]
fn spec_16_8_branch_switching_does_not_reload() {
    let (mut s, _) = scenario();
    s.loads_since();
    for _ in 0..6 {
        s.service.cycle_branch(Direction::Next);
        s.service.move_left();
        s.service.move_right();
        s.run();
    }
    s.service.go_back();
    s.service.go_forward();
    s.run();
    assert_eq!(s.loads_since(), 0);
}

#[test]
fn evicted_panels_keep_their_state_and_reload_once() {
    let (mut s, [home, documents, projects, downloads]) = scenario();
    s.service.set_capacity(2);
    s.service.set_visible(&[downloads]);
    s.run();
    s.service.move_cursor(downloads, 1);
    s.service.activate_panel(projects).unwrap();
    s.service.set_visible(&[projects]);
    s.run();
    // Only the two most recent listings stay; the others are unwatched.
    let cached = [home, documents, projects, downloads]
        .iter()
        .filter(|p| s.service.listing(**p).is_some())
        .count();
    assert_eq!(cached, 2);
    assert_eq!(s.watched.len(), 2);
    s.loads_since();
    s.service.activate_panel(home).unwrap();
    s.run();
    assert_eq!(s.loads_since(), 1);
    s.service.activate_panel(downloads).unwrap();
    s.run();
    assert_eq!(
        s.service.state(downloads).unwrap().cursor,
        Some(loc("/h/Downloads/big.iso")),
        "logical state survives eviction"
    );
}

// Navigation semantics (D3)

#[test]
fn sidebar_locations_open_as_new_root_branches() {
    let (mut s, [_, _, projects, _]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.service.open_location(loc("/h/Downloads"));
    s.run();
    let ws = s.service.workspace();
    assert!(ws.panel(s.active()).unwrap().is_root());
    assert_eq!(ws.roots().len(), 2);
    assert!(ws.panel(projects).unwrap().children().is_empty());
}

#[test]
fn breadcrumbs_activate_or_insert_ancestors() {
    let mut s = Shell::new();
    s.service.open_location(loc("/h/Documents/Projects"));
    s.run();
    let projects = s.active();
    s.service.open_ancestor(loc("/h"));
    s.run();
    let path: Vec<Location> = s
        .service
        .workspace()
        .active_path()
        .iter()
        .map(|&p| s.service.location(p).unwrap().clone())
        .collect();
    assert_eq!(
        path,
        [loc("/h"), loc("/h/Documents"), loc("/h/Documents/Projects")]
    );
    assert_eq!(s.service.active_location(), &loc("/h"));
    s.service.open_ancestor(loc("/h/Documents/Projects"));
    assert_eq!(s.active(), projects);
}

#[test]
fn parent_at_a_root_inserts_a_parent_panel() {
    let mut s = Shell::new();
    s.service.open_location(loc("/h/Documents"));
    s.run();
    assert!(s.service.go_parent());
    s.run();
    assert_eq!(s.service.active_location(), &loc("/h"));
}

// Availability (D14)

#[test]
fn a_deleted_folder_marks_its_panels_unavailable_but_keeps_them() {
    let (mut s, [_, documents, projects, downloads]) = scenario();
    let generation = s.watched[&documents];
    assert!(
        !s.service
            .directory_changed(documents, generation, DirectoryChange::Removed)
    );
    s.run();
    let ws = s.service.workspace();
    assert_eq!(
        ws.panel(documents).unwrap().unavailable(),
        Some(Unavailability::Missing)
    );
    assert_eq!(
        ws.panel(projects).unwrap().unavailable(),
        Some(Unavailability::Missing)
    );
    assert_eq!(ws.panel(downloads).unwrap().unavailable(), None);
    assert_eq!(ws.branch_count(), 2, "no branch is destroyed");
    assert!(!s.watched.contains_key(&documents));
    assert!(matches!(
        s.service.listing(documents).unwrap().content(),
        Content::Failed(ListingError::NotFound)
    ));
    // It comes back: Refresh retries and recovers.
    s.service.reload(documents);
    s.run();
    assert_eq!(
        s.service
            .workspace()
            .panel(documents)
            .unwrap()
            .unavailable(),
        None
    );
    assert!(s.watched.contains_key(&documents));
}

#[test]
fn detaching_a_device_stops_watching_its_panels() {
    let mut s = Shell::new();
    s.tree
        .insert("/media/usb".into(), vec![("f".into(), false)]);
    s.service.open_location(loc("/media/usb"));
    s.run();
    let usb = s.active();
    assert!(s.watched.contains_key(&usb));
    assert_eq!(
        s.service.detach_device(std::path::Path::new("/media/usb")),
        1
    );
    s.run();
    assert!(!s.watched.contains_key(&usb));
    assert_eq!(
        s.service.workspace().panel(usb).unwrap().unavailable(),
        Some(Unavailability::DeviceRemoved)
    );
}

#[test]
fn stale_listings_are_ignored() {
    let mut s = Shell::new();
    let home = s.active();
    s.service.reload(home);
    s.service.reload(home);
    let effects = s.service.take_effects();
    let generations: Vec<u64> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::Load { generation, .. } => Some(*generation),
            _ => None,
        })
        .collect();
    assert_eq!(generations.len(), 2);
    s.service.listing_loaded(home, generations[0], Ok(vec![]));
    assert_eq!(s.shown(home).len(), 3, "the superseded result was dropped");
}

// Cursor and selection

#[test]
fn cursor_movement_clamps_or_wraps() {
    let mut s = Shell::new();
    let home = s.active();
    assert_eq!(s.service.move_cursor(home, -1), Some(loc("/h/notes.txt")));
    assert_eq!(s.service.move_cursor(home, 5), Some(loc("/h/notes.txt")));
    assert_eq!(s.service.cursor_to_first(home), Some(loc("/h/Documents")));
    s.service.set_wrap_cursor(true);
    assert_eq!(s.service.move_cursor(home, -1), Some(loc("/h/notes.txt")));
}

#[test]
fn range_selection_covers_anchor_to_cursor() {
    let mut s = Shell::new();
    let home = s.active();
    s.service.cursor_to_last(home);
    s.service.start_range(home);
    s.service.move_cursor(home, -2);
    s.service.extend_range(home);
    assert_eq!(s.service.state(home).unwrap().selection.len(), 3);
    s.service.move_cursor(home, 1);
    s.service.extend_range(home);
    assert_eq!(
        s.service.state(home).unwrap().selection.items(),
        [loc("/h/Downloads"), loc("/h/notes.txt")]
    );
}

#[test]
fn spec_16_6_selection_spans_panels_and_branches() {
    let (mut s, [home, documents, projects, downloads]) = scenario();
    s.service
        .set_selection(projects, vec![loc("/h/Documents/Projects/x.rs")]);
    s.service.set_selection(home, vec![loc("/h/notes.txt")]);
    s.service
        .set_selection(downloads, vec![loc("/h/Downloads/big.iso")]);
    let groups = s.service.selection();
    let panels: Vec<PanelId> = groups.iter().map(|g| g.panel).collect();
    assert_eq!(
        panels,
        [home, projects, downloads],
        "spatial order, Home once"
    );
    assert!(s.service.targets().len() == 3);
    let _ = documents;
    assert!(s.service.clear_selections());
    assert!(s.service.selection().is_empty());
}

#[test]
fn without_a_selection_the_cursor_item_is_the_target() {
    let mut s = Shell::new();
    let home = s.active();
    assert!(s.service.targets().is_empty());
    s.service.cursor_to_last(home);
    assert_eq!(s.service.targets()[0].items, [loc("/h/notes.txt")]);
}

#[test]
fn closing_a_branch_drops_its_selection_and_watches_only() {
    let (mut s, [home, documents, projects, downloads]) = scenario();
    s.service
        .set_selection(projects, vec![loc("/h/Documents/Projects/x.rs")]);
    s.service
        .set_selection(downloads, vec![loc("/h/Downloads/big.iso")]);
    let docs_branch = s.service.workspace().branches_through(projects)[0];
    s.service.close_branch(docs_branch).unwrap();
    s.run();
    assert!(!s.watched.contains_key(&projects));
    assert!(!s.watched.contains_key(&documents));
    assert!(s.watched.contains_key(&home));
    let groups = s.service.selection();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].panel, downloads);
}

#[test]
fn a_created_item_is_selected_once_listed() {
    let mut s = Shell::new();
    let home = s.active();
    s.service.select_when_listed(home, loc("/h/New Folder"));
    s.tree
        .get_mut("/h")
        .unwrap()
        .push(("New Folder".into(), true));
    s.service.reload(home);
    s.run();
    let state = s.service.state(home).unwrap();
    assert_eq!(state.cursor, Some(loc("/h/New Folder")));
    assert_eq!(state.selection.items(), [loc("/h/New Folder")]);
}

// Spec §16.6 Cross-branch operations through the operation manager

fn manager() -> (
    Arc<MemoryFs>,
    OperationManager,
    mpsc::Receiver<crate::operations::OperationEvent>,
) {
    let fs = Arc::new(MemoryFs::new());
    for d in [
        "/h",
        "/h/Documents",
        "/h/Documents/Projects",
        "/h/Downloads",
    ] {
        fs.dir(d);
    }
    fs.file("/h/notes.txt", "n");
    fs.file("/h/Documents/a.txt", "a");
    fs.file("/h/Documents/Projects/x.rs", "x");
    fs.file("/h/Downloads/a.txt", "other a");
    let manager = OperationManager::new(fs.clone(), 2);
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    manager.subscribe(Box::new(move |e| {
        let _ = tx.lock().unwrap().send(e.clone());
    }));
    (fs, manager, rx)
}

fn wait_finished(
    rx: &mpsc::Receiver<crate::operations::OperationEvent>,
) -> seshat_domain::operation::Operation {
    loop {
        let event = rx.recv_timeout(Duration::from_secs(5)).expect("event");
        if event.operation.is_finished() {
            return event.operation;
        }
    }
}

#[test]
fn spec_16_6_files_from_several_branches_copy_into_one_destination() {
    let (mut s, [home, _, projects, downloads]) = scenario();
    let (fs, manager, rx) = manager();
    s.service.set_selection(home, vec![loc("/h/notes.txt")]);
    s.service
        .set_selection(projects, vec![loc("/h/Documents/Projects/x.rs")]);
    let plan = TransferPlan::new(
        TransferKind::Copy,
        &s.service.selection(),
        s.service.location(downloads).unwrap(),
    )
    .unwrap();
    assert!(plan.spans_panels());
    manager.submit(plan.request()).unwrap();
    let op = wait_finished(&rx);
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(fs.content("/h/Downloads/notes.txt").as_deref(), Some("n"));
    assert_eq!(fs.content("/h/Downloads/x.rs").as_deref(), Some("x"));
    assert_eq!(
        fs.content("/h/notes.txt").as_deref(),
        Some("n"),
        "copy keeps sources"
    );
}

#[test]
fn spec_16_6_moves_between_branches() {
    let (mut s, [_, _, projects, downloads]) = scenario();
    let (fs, manager, rx) = manager();
    s.service
        .set_selection(projects, vec![loc("/h/Documents/Projects/x.rs")]);
    let plan = TransferPlan::new(
        TransferKind::Move,
        &s.service.selection(),
        s.service.location(downloads).unwrap(),
    )
    .unwrap();
    manager.submit(plan.request()).unwrap();
    assert_eq!(wait_finished(&rx).status(), OperationStatus::Completed);
    assert!(fs.get("/h/Documents/Projects/x.rs").is_none());
    assert_eq!(fs.content("/h/Downloads/x.rs").as_deref(), Some("x"));
}

#[test]
fn spec_16_6_workspace_changes_do_not_alter_a_running_operation() {
    let (mut s, [home, _, projects, downloads]) = scenario();
    let (fs, manager, rx) = manager();
    fs.set_gate(false); // hold copies until we are done changing things
    s.service
        .set_selection(projects, vec![loc("/h/Documents/Projects/x.rs")]);
    let plan = TransferPlan::new(
        TransferKind::Copy,
        &s.service.selection(),
        s.service.location(downloads).unwrap(),
    )
    .unwrap();
    let id = manager.submit(plan.request()).unwrap();
    let submitted = manager.get(id).unwrap();

    // Switch branch, toggle Focus Mode, close the source branch.
    s.service.activate_panel(home).unwrap();
    s.service.toggle_focus_mode();
    s.service.cycle_branch(Direction::Next);
    let source_branch = s.service.workspace().branches_through(projects)[0];
    s.service.close_branch(source_branch).unwrap();
    s.run();

    let running = manager.get(id).unwrap();
    assert_eq!(running.sources, submitted.sources);
    assert_eq!(running.destination, submitted.destination);
    fs.set_gate(true);
    assert_eq!(wait_finished(&rx).status(), OperationStatus::Completed);
    assert_eq!(fs.content("/h/Downloads/x.rs").as_deref(), Some("x"));
}

#[test]
fn d15_same_named_sources_from_different_folders_never_overwrite() {
    let (fs, manager, rx) = manager();
    fs.dir("/h/Target");
    let plan = TransferPlan::new(
        TransferKind::Copy,
        &[
            SelectionGroup {
                panel: PanelId::from_raw(1),
                folder: loc("/h/Documents"),
                items: vec![loc("/h/Documents/a.txt")],
            },
            SelectionGroup {
                panel: PanelId::from_raw(2),
                folder: loc("/h/Downloads"),
                items: vec![loc("/h/Downloads/a.txt")],
            },
        ],
        &loc("/h/Target"),
    )
    .unwrap();
    manager.submit(plan.request()).unwrap();
    let op = wait_finished(&rx);
    // Without a UI resolver, the second conflicting item is skipped.
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(fs.content("/h/Target/a.txt").as_deref(), Some("a"));
}

// Context-aware navigation (2-CONTEXT-AWARE-NAVIGATION.md)

use seshat_domain::entities::DirectoryIdentity;
use seshat_domain::workspace::Resolution;

fn dir_with_identity(path: &str, inode: u64) -> FileEntry {
    FileEntry {
        identity: Some(DirectoryIdentity { device: 1, inode }),
        ..entry(path, true)
    }
}

#[test]
fn nav_open_entry_reports_how_a_folder_was_resolved() {
    let mut s = Shell::new();
    let home = s.active();
    let docs = s
        .service
        .open_entry(home, &entry("/h/Documents", true))
        .unwrap()
        .expect("a folder");
    assert_eq!(docs.resolution, Resolution::ContinueActiveBranch);
    s.run();
    s.service.move_left();
    let downloads = s
        .service
        .open_entry(home, &entry("/h/Downloads", true))
        .unwrap()
        .unwrap();
    assert_eq!(downloads.resolution, Resolution::CreateNewBranch);
    s.run();
    s.service.move_left();
    let again = s
        .service
        .open_entry(home, &entry("/h/Documents", true))
        .unwrap()
        .unwrap();
    assert_eq!(again.resolution, Resolution::ReactivateExistingBranch);
    assert_eq!(again.panel, docs.panel);
    // Files are launched, not navigated.
    assert!(
        s.service
            .open_entry(home, &entry("/h/notes.txt", false))
            .unwrap()
            .is_none()
    );
}

#[test]
fn nav_6_reactivation_reads_and_watches_nothing_new() {
    let (mut s, [home, documents, projects, _]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.service.cycle_branch(Direction::Next);
    s.service.move_left();
    s.run();
    s.loads_since();
    let watched = s.watched.len();
    let opened = s
        .service
        .open_entry(home, &entry("/h/Documents", true))
        .unwrap()
        .unwrap();
    s.run();
    assert_eq!(opened.resolution, Resolution::ReactivateExistingBranch);
    assert_eq!(s.active(), projects, "back at the remembered panel (C1)");
    assert_eq!(s.loads_since(), 0, "cached listings are reused");
    assert_eq!(s.watched.len(), watched, "no extra watches");
    let _ = documents;
}

#[test]
fn nav_16_3_reactivation_restores_panel_state() {
    let (mut s, [home, documents, projects, _]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.service.move_cursor(projects, 1);
    s.service.set_sort_key(documents, SortKey::Size);
    s.service.set_query(documents, "a");
    s.service.cycle_branch(Direction::Next);
    s.service.move_left();
    s.service
        .open_entry(home, &entry("/h/Documents", true))
        .unwrap();
    s.run();
    let docs = s.service.state(documents).unwrap();
    assert_eq!(docs.options.sort.key, SortKey::Size);
    assert_eq!(docs.options.query, "a");
    assert_eq!(
        s.service.state(projects).unwrap().cursor,
        Some(loc("/h/Documents/Projects/x.rs"))
    );
}

#[test]
fn nav_c3_a_renamed_folder_is_read_again_under_its_new_name() {
    let mut s = Shell::new();
    let home = s.active();
    s.tree.insert("/h/Old".into(), vec![("f".into(), false)]);
    let old = s
        .service
        .open_entry(home, &dir_with_identity("/h/Old", 42))
        .unwrap()
        .unwrap()
        .panel;
    s.run();
    assert!(s.watched.contains_key(&old));
    // Renamed elsewhere; the parent now lists "New" with the same inode.
    let files = s.tree.remove("/h/Old").unwrap();
    s.tree.insert("/h/New".into(), files);
    s.service.move_left();
    s.loads_since();
    let opened = s
        .service
        .open_entry(home, &dir_with_identity("/h/New", 42))
        .unwrap()
        .unwrap();
    s.run();
    assert_eq!(opened.panel, old, "the same panel follows the rename");
    assert_eq!(s.service.location(old), Some(&loc("/h/New")));
    assert_eq!(s.loads_since(), 1, "read again once, at the new path");
    assert!(s.service.listing(old).is_some_and(|l| l.total() == 1));
}

#[test]
fn nav_unavailable_panels_wait_for_retry() {
    let (mut s, [_, documents, _, _]) = scenario();
    let generation = s.watched[&documents];
    s.service
        .directory_changed(documents, generation, DirectoryChange::Removed);
    s.run();
    // Evicted, then shown again: it stays unavailable instead of being read.
    s.service.set_capacity(1);
    s.run();
    s.loads_since();
    s.service.set_visible(&[documents]);
    s.run();
    assert_eq!(s.loads_since(), 0);
    assert!(matches!(
        s.service.listing(documents).unwrap().content(),
        Content::Failed(ListingError::NotFound)
    ));
    // Retry reads it.
    s.service.reload(documents);
    s.run();
    assert_eq!(s.loads_since(), 1);
}

// Backward branch pruning (3-BACKWARD-BRANCH-PRUNING.md)

use seshat_domain::workspace::{PruneMotion, PruneRefusal, RestoreRefusal};

fn prune(s: &mut Shell, motion: PruneMotion) -> Result<PruneOutcome, PruneError> {
    let ws = s.service.workspace();
    let (branch, panel) = (ws.active_branch().id(), ws.active_panel());
    let outcome = s.service.prune_back(branch, panel, motion);
    s.service.workspace().check_invariants().unwrap();
    outcome
}

#[test]
fn prune_db_only_unwatches_removed_panels_and_saves() {
    let (mut s, [_, documents, projects, _]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.run();
    s.loads_since();
    let outcome = prune(&mut s, PruneMotion::Step).unwrap();
    assert_eq!(outcome.destination, documents);
    assert_eq!(outcome.removed, 1);
    assert!(!outcome.ended_branch);
    let effects = s.service.take_effects();
    assert!(
        effects.iter().all(
            |e| matches!(e, Effect::Unwatch { panel } if *panel == projects)
                || *e == Effect::Persist
        ),
        "workspace structure only, never files: {effects:?}"
    );
    assert!(
        effects.contains(&Effect::Unwatch { panel: projects }),
        "the pruned panel is no longer watched, and nothing is reloaded"
    );
    assert_eq!(s.active(), documents);
}

#[test]
fn prune_never_changes_files_or_running_operations() {
    let (mut s, [_, _, projects, downloads]) = scenario();
    let (fs, manager, rx) = manager();
    fs.set_gate(false);
    s.service
        .set_selection(projects, vec![loc("/h/Documents/Projects/x.rs")]);
    let plan = TransferPlan::new(
        TransferKind::Copy,
        &s.service.selection(),
        s.service.location(downloads).unwrap(),
    )
    .unwrap();
    let id = manager.submit(plan.request()).unwrap();
    let submitted = manager.get(id).unwrap();

    // Prune the source branch away entirely, then undo it.
    s.service.activate_panel(projects).unwrap();
    let outcome = prune(&mut s, PruneMotion::Segment).unwrap();
    assert!(outcome.ended_branch);
    assert_eq!(outcome.deselected, 1);
    assert!(
        s.service.selection().is_empty(),
        "the selection left with its panel"
    );
    s.run();
    assert_eq!(manager.get(id).unwrap().sources, submitted.sources);
    assert_eq!(
        fs.content("/h/Documents/Projects/x.rs").as_deref(),
        Some("x")
    );

    s.service.undo_workspace().unwrap();
    s.run();
    assert_eq!(
        s.service.selection().len(),
        1,
        "undo restores the selection"
    );
    fs.set_gate(true);
    assert_eq!(wait_finished(&rx).status(), OperationStatus::Completed);
    assert_eq!(
        fs.content("/h/Documents/Projects/x.rs").as_deref(),
        Some("x")
    );
}

#[test]
fn prune_and_undo_are_refused_in_focus_mode() {
    let (mut s, [_, _, projects, _]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.service.toggle_focus_mode();
    s.run();
    let before = s.service.workspace().clone();
    assert_eq!(prune(&mut s, PruneMotion::Step), Err(PruneError::FocusMode));
    assert_eq!(
        prune(&mut s, PruneMotion::Segment),
        Err(PruneError::FocusMode)
    );
    assert_eq!(s.service.workspace(), &before);
    assert_eq!(s.service.take_effects(), []);

    // Undo too, even with a change to undo; it stays available.
    s.service.toggle_focus_mode();
    prune(&mut s, PruneMotion::Step).unwrap();
    s.service.toggle_focus_mode();
    let before = s.service.workspace().clone();
    assert_eq!(s.service.undo_workspace(), Err(UndoError::FocusMode));
    assert_eq!(s.service.workspace(), &before);
    assert_eq!(s.service.undo_depth(), 1);
    s.service.toggle_focus_mode();
    assert!(s.service.undo_workspace().is_ok());
}

#[test]
fn prune_refusals_change_nothing() {
    let (mut s, [home, documents, _, _]) = scenario();
    s.service.activate_panel(documents).unwrap();
    s.run();
    let before = s.service.workspace().clone();
    assert_eq!(
        prune(&mut s, PruneMotion::Step),
        Err(PruneError::Refused(PruneRefusal::PanelsBeyond))
    );
    s.service.activate_panel(home).unwrap();
    let before_home = s.service.workspace().clone();
    assert_eq!(
        prune(&mut s, PruneMotion::Segment),
        Err(PruneError::Refused(PruneRefusal::Shared { branches: 2 }))
    );
    assert_eq!(s.service.workspace(), &before_home);
    assert_ne!(&before, &before_home, "only activation differed");
    assert_eq!(s.service.undo_depth(), 0);
}

#[test]
fn prune_undo_restores_panel_state_and_reloads_only_the_active_panel() {
    let (mut s, [_, documents, projects, _]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.run();
    s.service.move_cursor(projects, 1);
    s.service.set_sort_key(projects, SortKey::Size);
    s.service.set_query(projects, ".rs");
    let state = s.service.state(projects).unwrap().clone();
    let docs_state = s.service.state(documents).unwrap().clone();

    prune(&mut s, PruneMotion::Step).unwrap();
    s.run();
    assert_eq!(s.service.state(documents), Some(&docs_state));
    s.loads_since();

    let outcome = s.service.undo_workspace().unwrap();
    s.run();
    assert_eq!(outcome.restored, 1);
    assert_eq!(s.active(), projects, "same panel id, active again");
    assert_eq!(s.service.state(projects), Some(&state));
    assert_eq!(s.loads_since(), 1, "only the restored active panel is read");
    assert_eq!(s.service.undo_workspace(), Err(UndoError::NothingToUndo));
}

#[test]
fn prune_undo_that_no_longer_fits_is_dropped_and_older_changes_still_undo() {
    let (mut s, [home, documents, projects, downloads]) = scenario();
    // Prune Projects, then prune Downloads away (its branch ends).
    s.service.activate_panel(projects).unwrap();
    prune(&mut s, PruneMotion::Step).unwrap();
    s.service.activate_panel(downloads).unwrap();
    let outcome = prune(&mut s, PruneMotion::Step).unwrap();
    assert!(outcome.ended_branch);
    assert_eq!(s.active(), home);
    s.run();
    // Reopening Downloads makes its undo impossible.
    let reopened = s.open("Downloads");
    assert_ne!(reopened, downloads, "a fresh panel, not the pruned one");
    assert_eq!(
        s.service.undo_workspace(),
        Err(UndoError::Refused(RestoreRefusal::Changed))
    );
    assert_eq!(
        s.service.undo_depth(),
        1,
        "the impossible change was dropped"
    );
    s.service.undo_workspace().unwrap();
    s.run();
    assert_eq!(s.active(), projects);
    assert_eq!(
        s.service.workspace().panel(projects).unwrap().parent(),
        Some(documents)
    );
}

#[test]
fn prune_undo_history_is_bounded() {
    let mut s = Shell::new();
    for _ in 0..UNDO_LIMIT + 5 {
        s.open("Documents");
        prune(&mut s, PruneMotion::Step).unwrap();
    }
    assert_eq!(s.service.undo_depth(), UNDO_LIMIT);
}

#[test]
fn prune_keeps_l_h_and_branch_switching_working() {
    let (mut s, [home, documents, projects, downloads]) = scenario();
    s.service.activate_panel(projects).unwrap();
    s.service.move_cursor(projects, 1);
    prune(&mut s, PruneMotion::Step).unwrap();
    s.run();

    // `l` on the pruned folder continues the branch with a fresh panel.
    let e = entry("/h/Documents/Projects", true);
    let opened = s.service.open_entry(documents, &e).unwrap().unwrap();
    s.run();
    assert_eq!(opened.resolution, Resolution::ContinueActiveBranch);
    assert!(opened.created);
    assert_ne!(opened.panel, projects);
    assert_eq!(s.service.state(opened.panel).unwrap().cursor, None);

    // `h` and `[`/`]` behave as before.
    assert!(s.service.move_left());
    assert_eq!(s.active(), documents);
    s.service.cycle_branch(Direction::Next);
    s.run();
    assert_eq!(s.active(), downloads);
    s.service.cycle_branch(Direction::Next);
    assert_eq!(s.service.workspace().active_path()[0], home);
    assert_eq!(s.service.workspace().branch_count(), 2);
}

// Fuzzy Find results (Spec 4 §9, decisions F5 and F9)

fn found(path: &str, dir: bool, origin: Option<(&Shell, PanelId)>) -> SearchResult {
    use seshat_domain::search::{SearchMatch, SearchOrigin};
    let name = path.rsplit('/').next().unwrap().to_owned();
    SearchResult {
        location: loc(path),
        kind: if dir {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        relative_path: name.clone(),
        name,
        origin: origin.map(|(s, panel)| SearchOrigin {
            branch: *s
                .service
                .workspace()
                .branches_through(panel)
                .first()
                .unwrap(),
            panel,
        }),
        score: 1,
        name_match: SearchMatch::default(),
        path_match: SearchMatch::default(),
    }
}

#[test]
fn a_folder_result_opens_along_the_path_from_its_origin() {
    let mut s = Shell::new();
    let home = s.active();
    let result = found("/h/Documents/Projects", true, Some((&s, home)));
    s.service.open_result(&result);
    s.run();
    let ws = s.service.workspace();
    ws.check_invariants().unwrap();
    assert_eq!(s.service.active_location(), &loc("/h/Documents/Projects"));
    assert_eq!(ws.panel_count(), 3, "Home, Documents, Projects");
    assert_eq!(ws.branch_count(), 1, "the branch continued");
    assert!(ws.can_go_back());
    s.service.go_back();
    assert_eq!(s.service.active_panel(), home, "one step back");
}

#[test]
fn a_folder_result_reuses_open_panels_and_branches() {
    let (mut s, [home, documents, projects, _]) = scenario();
    let panels = s.service.workspace().panel_count();
    let branches = s.service.workspace().branch_count();
    let result = found("/h/Documents/Projects", true, Some((&s, home)));
    s.service.open_result(&result);
    s.run();
    let ws = s.service.workspace();
    ws.check_invariants().unwrap();
    assert_eq!(ws.panel_count(), panels, "no duplicate panels");
    assert_eq!(ws.branch_count(), branches, "no duplicate branches");
    assert_eq!(s.service.active_panel(), projects);
    assert_eq!(
        ws.panel(documents).unwrap().state.cursor,
        Some(loc("/h/Documents/Projects"))
    );
}

#[test]
fn a_file_result_is_launched_and_the_workspace_is_untouched() {
    let (mut s, [home, ..]) = scenario();
    let before = s.service.workspace().clone();
    let result = found("/h/Documents/a.txt", false, Some((&s, home)));
    s.service.open_result(&result);
    assert_eq!(
        s.service.take_effects(),
        [Effect::Launch(vec![loc("/h/Documents/a.txt")])]
    );
    assert_eq!(s.service.workspace(), &before);
}

#[test]
fn revealing_a_result_moves_the_cursor_and_keeps_the_selection() {
    let (mut s, [home, documents, ..]) = scenario();
    s.service
        .set_selection(documents, vec![loc("/h/Documents/b.txt")]);
    let result = found("/h/Documents/a.txt", false, Some((&s, home)));
    s.service.reveal_result(&result);
    s.run();
    assert_eq!(s.service.active_panel(), documents);
    let state = s.service.state(documents).unwrap();
    assert_eq!(state.cursor, Some(loc("/h/Documents/a.txt")));
    assert_eq!(
        state.selection.items(),
        [loc("/h/Documents/b.txt")],
        "the earlier selection is kept (F9)"
    );
}

#[test]
fn revealing_a_result_in_the_searched_folder_activates_that_panel() {
    let (mut s, [home, ..]) = scenario();
    let panels = s.service.workspace().panel_count();
    let result = found("/h/notes.txt", false, Some((&s, home)));
    s.service.reveal_result(&result);
    s.run();
    assert_eq!(s.service.active_panel(), home);
    assert_eq!(s.service.workspace().panel_count(), panels);
    assert_eq!(
        s.service.state(home).unwrap().cursor,
        Some(loc("/h/notes.txt"))
    );
}

#[test]
fn results_without_a_usable_origin_open_like_a_typed_location() {
    // No origin (a typed absolute path), a panel closed meanwhile, or a
    // panel the result isn't below: the address bar's rule (D3).
    for origin in [None, Some(PanelId::from_raw(999))] {
        let mut s = Shell::new();
        let result = SearchResult {
            origin: origin.map(|panel| seshat_domain::search::SearchOrigin {
                branch: s.service.workspace().active_branch().id(),
                panel,
            }),
            ..found("/h/Downloads", true, None)
        };
        s.service.open_result(&result);
        s.run();
        s.service.workspace().check_invariants().unwrap();
        assert_eq!(s.service.active_location(), &loc("/h/Downloads"));
        assert_eq!(
            s.service.workspace().branch_count(),
            2,
            "a new root branch, as from the address bar"
        );
    }
}

#[test]
fn a_result_from_another_branch_reactivates_it_in_focus_mode() {
    // Entire Workspace (§6.3): a result keeps the branch and panel whose
    // folder produced it. Both branches share Home, so it is searched once.
    let (mut s, [home, _, projects, downloads]) = scenario();
    s.service.toggle_focus_mode();
    assert_eq!(s.service.active_panel(), downloads);
    let roots = crate::search::roots(
        &seshat_domain::search::SearchScope::EntireWorkspace,
        s.service.workspace(),
    )
    .unwrap();
    assert_eq!(roots.len(), 1, "the shared Home is searched once");
    let origin = roots[0].origin.unwrap();
    assert_eq!(origin.panel, home);
    let (panels, branches) = (
        s.service.workspace().panel_count(),
        s.service.workspace().branch_count(),
    );

    let result = SearchResult {
        origin: Some(origin),
        ..found("/h/Documents/Projects/x.rs", false, None)
    };
    s.service.reveal_result(&result);
    s.run();
    let ws = s.service.workspace();
    ws.check_invariants().unwrap();
    assert_eq!(
        s.service.active_panel(),
        projects,
        "the other branch is back"
    );
    assert_eq!((ws.panel_count(), ws.branch_count()), (panels, branches));
    assert!(ws.focus_mode(), "Focus Mode changes presentation only");
    assert_eq!(
        s.service.state(projects).unwrap().cursor,
        Some(loc("/h/Documents/Projects/x.rs"))
    );
    s.service.go_back();
    assert_eq!(s.service.active_panel(), downloads, "one step back");
}
