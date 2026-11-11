//! File actions on the workspace's targets: what rename, Trash, delete,
//! restore, new item, empty Trash and the clipboard act on, and whether
//! they may run at all.
//!
//! Each function reads the [`WorkspaceService`] and returns an
//! [`ActionPlan`] (or a typed [`ActionRefusal`]). It never runs anything:
//! the presentation layer shows the confirmation or name prompt the plan
//! asks for and hands the captured [`OperationRequest`] to the operation
//! manager. Every source path is captured when the plan is made, so what
//! runs never depends on which panel is active later. Keys, menus and the
//! command line all go through these functions, so they share one set of
//! rules.

use std::path::PathBuf;

use seshat_domain::entities::Location;
use seshat_domain::workspace::PanelId;
use thiserror::Error;

use super::{SelectionGroup, WorkspaceService};
use crate::operations::{OperationRequest, top_level_trash_item, unique_path};

/// Why a file action cannot run. The messages are written for the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionRefusal {
    /// Nothing is selected and there is no cursor item. The user can see
    /// that, so the UI may stay silent.
    #[error("nothing is selected")]
    NothingSelected,
    #[error("select exactly one item to rename")]
    ExactlyOneItem,
    #[error("items in the Trash can't be renamed")]
    RenameInTrash,
    #[error("these items are already in the Trash")]
    AlreadyInTrash,
    #[error("items in the Trash can only be restored or deleted")]
    TrashItemsStay,
    /// The active panel is not a regular folder (it is in the Trash).
    #[error("folders can't be created here")]
    NotAFolder,
    #[error("only items at the top level of the Trash can be restored")]
    OnlyTopLevelTrash,
    #[error("open the Trash to empty it")]
    NotTrashRoot,
    /// The Trash is already empty; the UI may stay silent.
    #[error("the Trash is empty")]
    TrashEmpty,
}

impl ActionRefusal {
    /// Whether the user can already see why nothing happened, so no
    /// message is needed.
    pub fn is_silent(&self) -> bool {
        matches!(self, Self::NothingSelected | Self::TrashEmpty)
    }
}

/// What the user must confirm before a request is submitted. The UI turns
/// it into text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    /// Moving items from several panels to the Trash (decision D7).
    TrashAcrossPanels {
        count: usize,
        origins: Vec<(Location, usize)>,
    },
    /// Permanent deletion. `origins` is set when the items come from more
    /// than one panel.
    Delete {
        items: Vec<Location>,
        origins: Option<Vec<(Location, usize)>>,
    },
    EmptyTrash {
        count: usize,
    },
}

/// What a name is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Rename,
    NewFolder,
    NewFile,
}

/// A name to ask the user for. Its target was captured when the plan was
/// made; [`NamePrompt::complete`] builds the request from the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePrompt {
    pub kind: NameKind,
    /// The name the prompt starts with.
    pub suggestion: String,
    /// Pre-select only the part before the extension (files).
    pub select_stem: bool,
    panel: PanelId,
    /// The item to rename, or the folder to create in.
    target: PathBuf,
}

/// A request and the item to select once its panel lists it.
pub type Submission = (OperationRequest, Option<(PanelId, Location)>);

impl NamePrompt {
    /// The request for the name the user typed. The operation manager
    /// validates the name.
    pub fn complete(&self, name: String) -> Submission {
        match self.kind {
            NameKind::Rename => {
                let select = self
                    .target
                    .parent()
                    .map(|folder| (self.panel, Location::Path(folder.join(&name))));
                let request = OperationRequest::Rename {
                    path: self.target.clone(),
                    new_name: name,
                };
                (request, select)
            }
            NameKind::NewFolder | NameKind::NewFile => {
                create(self.kind, self.panel, self.target.clone(), name)
            }
        }
    }
}

/// What to do next for a file action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionPlan {
    /// Submit the request now.
    Submit {
        request: OperationRequest,
        select: Option<(PanelId, Location)>,
    },
    /// Ask first; submit the request if the user agrees.
    Confirm {
        request: OperationRequest,
        confirmation: Confirmation,
    },
    /// Ask for a name, then submit [`NamePrompt::complete`].
    AskName(NamePrompt),
}

/// A result that had to leave out items in the Trash, which can't take part
/// in the action. The UI says how many, so nothing is dropped silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partial<T> {
    pub value: T,
    pub left_out: usize,
}

/// The targets' regular paths for the clipboard (copy or cut). Items in the
/// Trash are left out and counted; if nothing else is selected, refused.
pub fn clipboard_sources(ws: &WorkspaceService) -> Result<Partial<Vec<PathBuf>>, ActionRefusal> {
    regular_paths(&ws.targets())
}

/// Moves the targets to the Trash; from several panels only after
/// confirmation (decision D7).
pub fn trash(ws: &WorkspaceService) -> Result<Partial<ActionPlan>, ActionRefusal> {
    let targets = ws.targets();
    let Partial {
        value: paths,
        left_out,
    } = regular_paths(&targets).map_err(|refusal| match refusal {
        ActionRefusal::TrashItemsStay => ActionRefusal::AlreadyInTrash,
        other => other,
    })?;
    let value = if targets.len() > 1 {
        ActionPlan::Confirm {
            confirmation: Confirmation::TrashAcrossPanels {
                count: paths.len(),
                origins: origins(&targets),
            },
            request: OperationRequest::Trash { paths },
        }
    } else {
        ActionPlan::Submit {
            request: OperationRequest::Trash { paths },
            select: None,
        }
    };
    Ok(Partial { value, left_out })
}

/// Deletes the targets permanently; always confirmed first.
pub fn delete(ws: &WorkspaceService) -> Result<ActionPlan, ActionRefusal> {
    let targets = ws.targets();
    let items: Vec<Location> = targets.iter().flat_map(|g| g.items.clone()).collect();
    if items.is_empty() {
        return Err(ActionRefusal::NothingSelected);
    }
    let origins = (targets.len() > 1).then(|| origins(&targets));
    Ok(ActionPlan::Confirm {
        request: OperationRequest::Delete {
            items: items.clone(),
        },
        confirmation: Confirmation::Delete { items, origins },
    })
}

/// Restores the targets that are at the top level of the Trash.
pub fn restore(ws: &WorkspaceService) -> Result<ActionPlan, ActionRefusal> {
    let items: Vec<PathBuf> = ws
        .targets()
        .into_iter()
        .flat_map(|g| g.items)
        .filter_map(|l| match l {
            Location::Trash(rel) if top_level_trash_item(&rel).is_ok() => Some(rel),
            _ => None,
        })
        .collect();
    if items.is_empty() {
        return Err(ActionRefusal::OnlyTopLevelTrash);
    }
    Ok(ActionPlan::Submit {
        request: OperationRequest::Restore { items },
        select: None,
    })
}

/// Renames the single target: to `name` when given (command line),
/// otherwise after asking for one.
pub fn rename(ws: &WorkspaceService, name: Option<String>) -> Result<ActionPlan, ActionRefusal> {
    let targets = ws.targets();
    let [group] = targets.as_slice() else {
        return Err(if targets.is_empty() {
            ActionRefusal::NothingSelected
        } else {
            ActionRefusal::ExactlyOneItem
        });
    };
    let [item] = group.items.as_slice() else {
        return Err(ActionRefusal::ExactlyOneItem);
    };
    let Location::Path(path) = item else {
        return Err(ActionRefusal::RenameInTrash);
    };
    let entry = ws.listing(group.panel).and_then(|l| l.entry(item));
    let (suggestion, is_file) = match entry {
        Some(e) => (e.display_name.clone(), !e.is_directory()),
        None => (
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            true,
        ),
    };
    let prompt = NamePrompt {
        kind: NameKind::Rename,
        suggestion,
        select_stem: is_file,
        panel: group.panel,
        target: path.clone(),
    };
    Ok(match name {
        Some(name) => submit(prompt.complete(name)),
        None => ActionPlan::AskName(prompt),
    })
}

/// Creates a folder (`directory`) or an empty file in the active panel:
/// named `name` when given (command line), otherwise after asking, with a
/// free name suggested from what the panel lists.
pub fn new_item(
    ws: &WorkspaceService,
    directory: bool,
    name: Option<String>,
) -> Result<ActionPlan, ActionRefusal> {
    let Location::Path(parent) = ws.active_location() else {
        return Err(ActionRefusal::NotAFolder);
    };
    let panel = ws.active_panel();
    let kind = if directory {
        NameKind::NewFolder
    } else {
        NameKind::NewFile
    };
    if let Some(name) = name {
        return Ok(submit(create(kind, panel, parent.clone(), name)));
    }
    let base = if directory { "New Folder" } else { "New File" };
    let listing = ws.listing(panel);
    let taken = |p: &std::path::Path| {
        listing.is_some_and(|l| l.entries().iter().any(|e| e.local_path() == Some(p)))
    };
    let first = parent.join(base);
    let free = if taken(&first) {
        unique_path(&first, directory, taken)
    } else {
        first
    };
    let suggestion = free
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| base.to_owned());
    Ok(ActionPlan::AskName(NamePrompt {
        kind,
        suggestion,
        select_stem: false,
        panel,
        target: parent.clone(),
    }))
}

/// Deletes everything in the Trash; only from the Trash's own panel, and
/// always confirmed first.
pub fn empty_trash(ws: &WorkspaceService) -> Result<ActionPlan, ActionRefusal> {
    if ws.active_location() != &Location::trash_root() {
        return Err(ActionRefusal::NotTrashRoot);
    }
    let items: Vec<Location> = ws
        .listing(ws.active_panel())
        .map(|l| l.entries().iter().map(|e| e.location.clone()).collect())
        .unwrap_or_default();
    if items.is_empty() {
        return Err(ActionRefusal::TrashEmpty);
    }
    Ok(ActionPlan::Confirm {
        confirmation: Confirmation::EmptyTrash { count: items.len() },
        request: OperationRequest::Delete { items },
    })
}

/// File actions offered in menus and toolbars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    Open,
    CopyPath,
    Cut,
    Copy,
    Paste,
    Rename,
    Trash,
    Delete,
    Restore,
    NewFolder,
    NewFile,
    EmptyTrash,
}

/// What decides whether a [`FileAction`] is offered: the number of targets
/// and what kind of location the active panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Availability {
    /// Items the actions would act on (the selection, or the cursor item).
    pub selected: usize,
    /// The active panel is in the Trash.
    pub in_trash: bool,
    /// The active panel is a regular folder.
    pub in_folder: bool,
}

impl Availability {
    pub fn of(ws: &WorkspaceService) -> Self {
        let current = ws.active_location();
        Self {
            selected: ws.targets().iter().map(|g| g.items.len()).sum(),
            in_trash: current.is_trash(),
            in_folder: matches!(current, Location::Path(_)),
        }
    }

    pub fn allows(self, action: FileAction) -> bool {
        let any = self.selected > 0;
        match action {
            FileAction::Open | FileAction::CopyPath | FileAction::Delete => any,
            FileAction::Cut | FileAction::Copy | FileAction::Trash => any && self.in_folder,
            FileAction::Rename => self.selected == 1 && self.in_folder,
            FileAction::Restore => any && self.in_trash,
            FileAction::Paste | FileAction::NewFolder | FileAction::NewFile => self.in_folder,
            FileAction::EmptyTrash => self.in_trash,
        }
    }
}

fn submit((request, select): Submission) -> ActionPlan {
    ActionPlan::Submit { request, select }
}

fn create(kind: NameKind, panel: PanelId, parent: PathBuf, name: String) -> Submission {
    let select = Some((panel, Location::Path(parent.join(&name))));
    let request = if kind == NameKind::NewFolder {
        OperationRequest::CreateDirectory { parent, name }
    } else {
        OperationRequest::CreateFile { parent, name }
    };
    (request, select)
}

/// Source folders and how many targets come from each, in order.
fn origins(targets: &[SelectionGroup]) -> Vec<(Location, usize)> {
    targets
        .iter()
        .map(|g| (g.folder.clone(), g.items.len()))
        .collect()
}

/// The targets' regular paths, counting the Trash items left out.
fn regular_paths(targets: &[SelectionGroup]) -> Result<Partial<Vec<PathBuf>>, ActionRefusal> {
    let mut paths = Vec::new();
    let mut left_out = 0;
    for item in targets.iter().flat_map(|g| &g.items) {
        match item {
            Location::Path(path) => paths.push(path.clone()),
            Location::Trash(_) => left_out += 1,
        }
    }
    match (paths.is_empty(), left_out) {
        (true, 0) => Err(ActionRefusal::NothingSelected),
        (true, _) => Err(ActionRefusal::TrashItemsStay),
        _ => Ok(Partial {
            value: paths,
            left_out,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use seshat_domain::entities::{EntryKind, FileEntry, Location};
    use seshat_domain::workspace::{PanelId, PanelState};

    use super::*;
    use crate::workspace::{Effect, TransferKind, TransferPlan};

    fn loc(s: &str) -> Location {
        Location::Path(s.into())
    }

    fn entry(location: Location, dir: bool) -> FileEntry {
        let name = match &location {
            Location::Path(p) | Location::Trash(p) => p.file_name().unwrap_or_default().to_owned(),
        };
        FileEntry {
            display_name: name.to_string_lossy().into_owned(),
            name,
            location,
            kind: if dir {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            size: None,
            modified: None,
            hidden: false,
            content_type: None,
            trash_info: None,
            identity: None,
        }
    }

    /// The folders the fake filesystem knows: `/h` and the Trash root.
    fn listing(location: &Location) -> Vec<FileEntry> {
        match location {
            Location::Path(p) if p == &PathBuf::from("/h") => vec![
                entry(loc("/h/Docs"), true),
                entry(loc("/h/New Folder"), true),
                entry(loc("/h/a.txt"), false),
                entry(loc("/h/b.txt"), false),
            ],
            Location::Path(p) if p == &PathBuf::from("/h/Docs") => {
                vec![entry(loc("/h/Docs/c.txt"), false)]
            }
            Location::Trash(rel) if rel.as_os_str().is_empty() => vec![
                entry(Location::Trash("old.txt".into()), false),
                entry(Location::Trash("dir".into()), true),
            ],
            _ => Vec::new(),
        }
    }

    fn run(ws: &mut WorkspaceService) {
        loop {
            let effects = ws.take_effects();
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                if let Effect::Load {
                    panel,
                    location,
                    generation,
                } = effect
                {
                    ws.listing_loaded(panel, generation, Ok(listing(&location)));
                }
            }
        }
    }

    fn home() -> WorkspaceService {
        let mut ws = WorkspaceService::starting_at(loc("/h"), PanelState::default());
        run(&mut ws);
        ws
    }

    fn open(ws: &mut WorkspaceService, location: Location) -> PanelId {
        ws.open_location(location);
        run(ws);
        ws.active_panel()
    }

    fn select(ws: &mut WorkspaceService, panel: PanelId, items: &[Location]) {
        ws.set_selection(panel, items.to_vec());
    }

    #[test]
    fn nothing_selected_is_refused_silently() {
        let ws = home();
        for result in [
            trash(&ws).map(|p| p.value),
            delete(&ws),
            rename(&ws, None),
            clipboard_sources(&ws).map(|_| unreachable!()),
        ] {
            let refusal = result.unwrap_err();
            assert_eq!(refusal, ActionRefusal::NothingSelected);
            assert!(refusal.is_silent());
        }
    }

    #[test]
    fn rename_needs_exactly_one_regular_item() {
        let mut ws = home();
        let home = ws.active_panel();
        select(&mut ws, home, &[loc("/h/a.txt"), loc("/h/b.txt")]);
        assert_eq!(rename(&ws, None), Err(ActionRefusal::ExactlyOneItem));

        // One item in each of two panels is still two items.
        let docs = open(&mut ws, loc("/h/Docs"));
        select(&mut ws, home, &[loc("/h/a.txt")]);
        select(&mut ws, docs, &[loc("/h/Docs/c.txt")]);
        assert_eq!(rename(&ws, None), Err(ActionRefusal::ExactlyOneItem));

        let mut ws = super::tests::home();
        let trash = open(&mut ws, Location::trash_root());
        select(&mut ws, trash, &[Location::Trash("old.txt".into())]);
        // The dialog and `:rename <name>` give the same answer.
        assert_eq!(rename(&ws, None), Err(ActionRefusal::RenameInTrash));
        assert_eq!(
            rename(&ws, Some("new".into())),
            Err(ActionRefusal::RenameInTrash)
        );
    }

    #[test]
    fn rename_asks_for_a_name_and_selects_the_result() {
        let mut ws = home();
        let home = ws.active_panel();
        select(&mut ws, home, &[loc("/h/a.txt")]);
        let Ok(ActionPlan::AskName(prompt)) = rename(&ws, None) else {
            panic!("expected a name prompt");
        };
        assert_eq!(prompt.kind, NameKind::Rename);
        assert_eq!(prompt.suggestion, "a.txt");
        assert!(prompt.select_stem, "files pre-select the stem");
        let (request, select) = prompt.complete("z.txt".into());
        assert_eq!(
            request,
            OperationRequest::Rename {
                path: "/h/a.txt".into(),
                new_name: "z.txt".into()
            }
        );
        assert_eq!(select, Some((home, loc("/h/z.txt"))));

        // With the name given, it is submitted directly, the same way.
        assert_eq!(
            rename(&ws, Some("z.txt".into())),
            Ok(ActionPlan::Submit {
                request,
                select: Some((home, loc("/h/z.txt")))
            })
        );
    }

    #[test]
    fn trash_from_one_panel_submits_and_from_several_confirms() {
        let mut ws = home();
        let home = ws.active_panel();
        select(&mut ws, home, &[loc("/h/a.txt")]);
        let plan = trash(&ws).unwrap();
        assert_eq!(plan.left_out, 0);
        assert_eq!(
            plan.value,
            ActionPlan::Submit {
                request: OperationRequest::Trash {
                    paths: vec!["/h/a.txt".into()]
                },
                select: None
            }
        );

        let docs = open(&mut ws, loc("/h/Docs"));
        select(&mut ws, docs, &[loc("/h/Docs/c.txt")]);
        let ActionPlan::Confirm {
            request,
            confirmation,
        } = trash(&ws).unwrap().value
        else {
            panic!("several panels must confirm (D7)");
        };
        assert_eq!(
            request,
            OperationRequest::Trash {
                paths: vec!["/h/a.txt".into(), "/h/Docs/c.txt".into()]
            }
        );
        assert_eq!(
            confirmation,
            Confirmation::TrashAcrossPanels {
                count: 2,
                origins: vec![(loc("/h"), 1), (loc("/h/Docs"), 1)]
            }
        );
    }

    #[test]
    fn trash_items_are_left_out_and_counted_never_dropped_silently() {
        let mut ws = home();
        let home = ws.active_panel();
        select(&mut ws, home, &[loc("/h/a.txt")]);
        let trash_panel = open(&mut ws, Location::trash_root());
        select(&mut ws, trash_panel, &[Location::Trash("old.txt".into())]);

        let sources = clipboard_sources(&ws).unwrap();
        assert_eq!(sources.value, vec![PathBuf::from("/h/a.txt")]);
        assert_eq!(sources.left_out, 1);
        assert_eq!(trash(&ws).unwrap().left_out, 1);

        // Only Trash items: refused, with a reason.
        ws.set_selection(home, Vec::new());
        assert_eq!(
            clipboard_sources(&ws).unwrap_err(),
            ActionRefusal::TrashItemsStay
        );
        let refusal = trash(&ws).unwrap_err();
        assert_eq!(refusal, ActionRefusal::AlreadyInTrash);
        assert!(!refusal.is_silent());
    }

    #[test]
    fn delete_always_confirms_and_lists_origins_across_panels() {
        let mut ws = home();
        let home = ws.active_panel();
        select(&mut ws, home, &[loc("/h/a.txt")]);
        assert_eq!(
            delete(&ws),
            Ok(ActionPlan::Confirm {
                request: OperationRequest::Delete {
                    items: vec![loc("/h/a.txt")]
                },
                confirmation: Confirmation::Delete {
                    items: vec![loc("/h/a.txt")],
                    origins: None
                }
            })
        );
        let docs = open(&mut ws, loc("/h/Docs"));
        select(&mut ws, docs, &[loc("/h/Docs/c.txt")]);
        let Ok(ActionPlan::Confirm {
            confirmation: Confirmation::Delete { origins, .. },
            ..
        }) = delete(&ws)
        else {
            panic!("delete must confirm");
        };
        assert_eq!(origins, Some(vec![(loc("/h"), 1), (loc("/h/Docs"), 1)]));
    }

    #[test]
    fn restore_takes_top_level_trash_items_only() {
        let mut ws = home();
        let home = ws.active_panel();
        select(&mut ws, home, &[loc("/h/a.txt")]);
        assert_eq!(restore(&ws), Err(ActionRefusal::OnlyTopLevelTrash));

        let trash = open(&mut ws, Location::trash_root());
        select(
            &mut ws,
            trash,
            &[
                Location::Trash("old.txt".into()),
                Location::Trash("dir/inner.txt".into()),
            ],
        );
        assert_eq!(
            restore(&ws),
            Ok(ActionPlan::Submit {
                request: OperationRequest::Restore {
                    items: vec!["old.txt".into()]
                },
                select: None
            })
        );
    }

    #[test]
    fn new_items_suggest_a_free_name_and_select_the_result() {
        let ws = home();
        let home = ws.active_panel();
        let Ok(ActionPlan::AskName(folder)) = new_item(&ws, true, None) else {
            panic!("expected a name prompt");
        };
        assert_eq!(folder.kind, NameKind::NewFolder);
        assert_eq!(folder.suggestion, "New Folder (2)", "New Folder is taken");
        assert!(!folder.select_stem);
        assert_eq!(
            folder.complete("Photos".into()),
            (
                OperationRequest::CreateDirectory {
                    parent: "/h".into(),
                    name: "Photos".into()
                },
                Some((home, loc("/h/Photos")))
            )
        );

        let Ok(ActionPlan::AskName(file)) = new_item(&ws, false, None) else {
            panic!("expected a name prompt");
        };
        assert_eq!(file.suggestion, "New File");
        assert_eq!(
            new_item(&ws, false, Some("x.md".into())),
            Ok(ActionPlan::Submit {
                request: OperationRequest::CreateFile {
                    parent: "/h".into(),
                    name: "x.md".into()
                },
                select: Some((home, loc("/h/x.md")))
            })
        );

        let mut ws = super::tests::home();
        open(&mut ws, Location::trash_root());
        assert_eq!(new_item(&ws, true, None), Err(ActionRefusal::NotAFolder));
    }

    #[test]
    fn empty_trash_only_from_the_trash_and_always_confirmed() {
        let mut ws = home();
        assert_eq!(empty_trash(&ws), Err(ActionRefusal::NotTrashRoot));
        open(&mut ws, Location::trash_root());
        assert_eq!(
            empty_trash(&ws),
            Ok(ActionPlan::Confirm {
                request: OperationRequest::Delete {
                    items: vec![
                        Location::Trash("old.txt".into()),
                        Location::Trash("dir".into())
                    ]
                },
                confirmation: Confirmation::EmptyTrash { count: 2 }
            })
        );
    }

    #[test]
    fn availability_matches_the_menu_rules() {
        let folder_one = Availability {
            selected: 1,
            in_trash: false,
            in_folder: true,
        };
        let folder_many = Availability {
            selected: 3,
            ..folder_one
        };
        let trash_one = Availability {
            selected: 1,
            in_trash: true,
            in_folder: false,
        };
        let nothing = Availability {
            selected: 0,
            ..folder_one
        };
        assert!(folder_one.allows(FileAction::Rename));
        assert!(!folder_many.allows(FileAction::Rename));
        assert!(folder_many.allows(FileAction::Trash));
        assert!(!folder_one.allows(FileAction::Restore));
        assert!(trash_one.allows(FileAction::Restore));
        assert!(trash_one.allows(FileAction::Delete));
        assert!(trash_one.allows(FileAction::EmptyTrash));
        assert!(!trash_one.allows(FileAction::Trash), "already in the Trash");
        assert!(!trash_one.allows(FileAction::Paste));
        assert!(!trash_one.allows(FileAction::Cut));
        assert!(!nothing.allows(FileAction::Delete));
        assert!(!nothing.allows(FileAction::Open));
        assert!(nothing.allows(FileAction::Paste));
        assert!(nothing.allows(FileAction::NewFolder));

        // Read from the workspace: the cursor item counts as a target.
        let mut ws = home();
        let home = ws.active_panel();
        assert_eq!(Availability::of(&ws), nothing);
        select(&mut ws, home, &[loc("/h/a.txt"), loc("/h/b.txt")]);
        assert_eq!(Availability::of(&ws).selected, 2);
        open(&mut ws, Location::trash_root());
        let in_trash = Availability::of(&ws);
        assert!(in_trash.in_trash && !in_trash.in_folder);
    }

    #[test]
    fn loose_paths_are_grouped_by_folder_in_first_seen_order() {
        let plan = TransferPlan::from_paths(
            TransferKind::Copy,
            &["/a/1".into(), "/b/2".into(), "/a/3".into()],
            &loc("/dest"),
        )
        .unwrap();
        // Sources follow their folders, as the confirmation lists them.
        assert_eq!(
            plan.sources,
            vec![
                PathBuf::from("/a/1"),
                PathBuf::from("/a/3"),
                PathBuf::from("/b/2")
            ]
        );
        assert_eq!(plan.origins, vec![(loc("/a"), 2), (loc("/b"), 1)]);
        assert!(plan.spans_panels());
    }
}
