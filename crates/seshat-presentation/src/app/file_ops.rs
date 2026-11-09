//! File-operation commands. Every change goes through the
//! [`OperationManager`](seshat_application::operations::OperationManager);
//! the UI only builds requests, asks for confirmation and shows results.
//!
//! Sources are the workspace-wide selection (or the active panel's cursor
//! item), captured as absolute paths when the request is built. A
//! destination is always explicit: the panel pasted or dropped into, or a
//! folder named on the command line. Nothing depends on which panel
//! happens to be active when an operation runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gtk::prelude::{ApplicationExt, GtkWindowExt};
use relm4::ComponentSender;
use seshat_application::operations::{
    OperationEvent, OperationEventKind, OperationRequest, unique_path,
};
use seshat_application::workspace::{SelectionGroup, TransferKind, TransferPlan};
use seshat_domain::entities::Location;
use seshat_domain::operation::OperationKind;
use seshat_domain::workspace::PanelId;

use super::workspace::capitalise;
use super::{App, Msg, Staged};
use crate::clipboard::{self, ClipboardFiles};
use crate::dialogs;

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_owned()
    } else {
        format!("{n} {many}")
    }
}

fn quoted_or_count(items: &[Location]) -> String {
    match items {
        [single] => format!("“{}”", crate::workspace_view::folder_title(single)),
        many => format!("{} items", many.len()),
    }
}

impl App {
    /// The active panel's folder, if it is a regular folder.
    fn active_folder(&self) -> Option<PathBuf> {
        match self.service.active_location() {
            Location::Path(path) => Some(path.clone()),
            Location::Trash(_) => None,
        }
    }

    fn target_items(&self) -> Vec<Location> {
        self.service
            .targets()
            .into_iter()
            .flat_map(|g| g.items)
            .collect()
    }

    fn target_paths(&self) -> Vec<PathBuf> {
        self.target_items()
            .into_iter()
            .filter_map(|l| match l {
                Location::Path(p) => Some(p),
                Location::Trash(_) => None,
            })
            .collect()
    }

    fn spans_panels(&self) -> bool {
        self.service.targets().len() > 1
    }

    pub(super) fn submit(
        &mut self,
        request: OperationRequest,
        select: Option<(PanelId, Location)>,
    ) {
        match self.operations.submit(request) {
            Ok(_) => {
                if let Some((panel, location)) = select {
                    self.service.select_when_listed(panel, location);
                }
            }
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
        }
    }

    /// Copy or cut: the targets go to the system clipboard, interoperable
    /// with other file managers (decision D9). The selection is consumed.
    pub(super) fn clipboard_store(&mut self, cut: bool) {
        let paths = self.target_paths();
        if paths.is_empty() {
            return;
        }
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        clipboard::set_files(&display, &paths, cut);
        self.staged = Some(Staged {
            cut,
            count: paths.len(),
        });
        self.service.clear_selections();
        self.keys.borrow_mut().interaction.reset();
    }

    pub(super) fn paste(&mut self, sender: &ComponentSender<Self>) {
        tracing::debug!("paste requested");
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let sender = sender.clone();
        relm4::spawn_local(async move {
            let files = clipboard::read_files(&display).await;
            sender.input_sender().emit(Msg::PasteFiles(files));
        });
    }

    /// Pastes into the active panel: the destination the user chose by
    /// going there.
    pub(super) fn paste_files(
        &mut self,
        files: Option<ClipboardFiles>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(files) = files else {
            self.notice = Some("The clipboard has no files to paste.".to_owned());
            return;
        };
        let Some(destination) = self.active_folder() else {
            self.notice = Some("Files can't be pasted here.".to_owned());
            return;
        };
        let kind = if files.cut {
            TransferKind::Move
        } else {
            TransferKind::Copy
        };
        let plan = TransferPlan::new(
            kind,
            &groups_by_folder(&files.paths),
            &Location::Path(destination),
        );
        match plan {
            Ok(plan) if plan.is_noop() => {}
            Ok(plan) => {
                // A cut is consumed by pasting it once.
                if files.cut
                    && let Some(display) = gtk::gdk::Display::default()
                {
                    clipboard::clear(&display);
                    self.staged = None;
                }
                self.confirm_transfer(plan, sender);
            }
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
        }
    }

    pub(super) fn rename_selection(&mut self, sender: &ComponentSender<Self>) {
        let targets = self.service.targets();
        let [group] = targets.as_slice() else {
            self.notice = Some("Select exactly one item to rename.".to_owned());
            return;
        };
        let [Location::Path(path)] = group.items.as_slice() else {
            return;
        };
        let panel = group.panel;
        let entry = self
            .service
            .listing(panel)
            .and_then(|l| l.entry(&group.items[0]))
            .cloned();
        let (name, is_file) = match &entry {
            Some(e) => (e.display_name.clone(), !e.is_directory()),
            None => (
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                true,
            ),
        };
        let path = path.clone();
        let sender = sender.clone();
        dialogs::name_prompt(
            &self.ui.window,
            "Rename",
            "Rename",
            &name,
            is_file,
            move |new_name| {
                let select = path
                    .parent()
                    .map(|p| (panel, Location::Path(p.join(&new_name))));
                let request = OperationRequest::Rename {
                    path: path.clone(),
                    new_name,
                };
                sender.input_sender().emit(Msg::Submit(request, select));
            },
        );
    }

    /// Moves the targets to the Trash. When they come from several panels,
    /// the user confirms first (decision D7).
    pub(super) fn trash_selection(&mut self, sender: &ComponentSender<Self>) {
        let paths = self.target_paths();
        if paths.is_empty() {
            return;
        }
        if !self.spans_panels() {
            self.submit(OperationRequest::Trash { paths }, None);
            return;
        }
        let details = self.describe_targets();
        let sender = sender.clone();
        dialogs::confirm(
            &self.ui.window,
            &format!(
                "Move {} to the Trash?",
                plural(paths.len(), "1 item", "items")
            ),
            &format!("The items come from several panels:\n{details}"),
            "Move to Trash",
            move || sender.input_sender().emit(Msg::TrashPaths(paths.clone())),
        );
    }

    pub(super) fn delete_selection(&mut self, sender: &ComponentSender<Self>) {
        let items = self.target_items();
        if items.is_empty() {
            return;
        }
        let detail = if self.spans_panels() {
            format!(
                "The items come from several panels:\n{}\n\nDeleted items are not moved to the Trash and cannot be recovered.",
                self.describe_targets()
            )
        } else {
            "Deleted items are not moved to the Trash and cannot be recovered.".to_owned()
        };
        let sender = sender.clone();
        dialogs::confirm(
            &self.ui.window,
            &format!("Permanently delete {}?", quoted_or_count(&items)),
            &detail,
            "Delete Permanently",
            move || {
                sender.input_sender().emit(Msg::Submit(
                    OperationRequest::Delete {
                        items: items.clone(),
                    },
                    None,
                ))
            },
        );
    }

    fn describe_targets(&self) -> String {
        self.service
            .targets()
            .iter()
            .map(|g| {
                format!(
                    "• {} from {}",
                    g.items.len(),
                    crate::pathbar::address_text(&g.folder)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn restore_selection(&mut self) {
        let items: Vec<PathBuf> = self
            .target_items()
            .into_iter()
            .filter_map(|l| match l {
                Location::Trash(rel) if rel.components().count() == 1 => Some(rel),
                _ => None,
            })
            .collect();
        if items.is_empty() {
            self.notice =
                Some("Only items at the top level of the Trash can be restored.".to_owned());
            return;
        }
        self.submit(OperationRequest::Restore { items }, None);
    }

    /// Creates a folder or file in the active panel. With `name` (from the
    /// command line) it is created directly; otherwise a name is asked for.
    pub(super) fn new_item(
        &mut self,
        directory: bool,
        name: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(parent) = self.active_folder() else {
            self.notice = Some("Folders can't be created here.".to_owned());
            return;
        };
        let panel = self.service.active_panel();
        let request = move |parent: PathBuf, name: String| {
            if directory {
                OperationRequest::CreateDirectory { parent, name }
            } else {
                OperationRequest::CreateFile { parent, name }
            }
        };
        if let Some(name) = name {
            let select = Some((panel, Location::Path(parent.join(&name))));
            self.submit(request(parent, name), select);
            return;
        }
        let base = if directory { "New Folder" } else { "New File" };
        // Suggest a free name based on what is already listed.
        let listing = self.service.listing(panel);
        let taken = |p: &Path| {
            listing.is_some_and(|l| l.entries().iter().any(|e| e.local_path() == Some(p)))
        };
        let suggestion = {
            let first = parent.join(base);
            let path = if taken(&first) {
                unique_path(&first, directory, taken)
            } else {
                first
            };
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| base.to_owned())
        };
        let sender = sender.clone();
        dialogs::name_prompt(
            &self.ui.window,
            base,
            "Create",
            &suggestion,
            false,
            move |name| {
                let select = Some((panel, Location::Path(parent.join(&name))));
                sender
                    .input_sender()
                    .emit(Msg::Submit(request(parent.clone(), name), select));
            },
        );
    }

    pub(super) fn empty_trash(&mut self, sender: &ComponentSender<Self>) {
        let panel = self.service.active_panel();
        if self.service.active_location() != &Location::trash_root() {
            self.notice = Some("Open the Trash to empty it.".to_owned());
            return;
        }
        let items: Vec<Location> = self
            .service
            .listing(panel)
            .map(|l| l.entries().iter().map(|e| e.location.clone()).collect())
            .unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let sender = sender.clone();
        dialogs::confirm(
            &self.ui.window,
            "Empty the Trash?",
            &format!(
                "{} will be permanently deleted. This cannot be undone.",
                plural(items.len(), "1 item", "items")
            ),
            "Empty Trash",
            move || {
                sender.input_sender().emit(Msg::Submit(
                    OperationRequest::Delete {
                        items: items.clone(),
                    },
                    None,
                ))
            },
        );
    }

    /// Files dropped on `panel`: into the folder row they were dropped on,
    /// or the panel's own folder. Dragging between panels works the same
    /// way as dragging in from another application.
    pub(super) fn drop_files(
        &mut self,
        panel: PanelId,
        paths: Vec<PathBuf>,
        into: Option<u32>,
        copy: bool,
    ) {
        let row = into
            .and_then(|position| {
                self.ui
                    .workspace
                    .with_panel(panel, |v| v.files.entry(position))
                    .flatten()
            })
            .and_then(|e| e.local_path().map(Path::to_path_buf));
        let folder = match self.service.location(panel) {
            Some(Location::Path(p)) => Some(p.clone()),
            _ => None,
        };
        let Some(destination) = row.or(folder) else {
            self.notice = Some("Files can't be dropped here.".to_owned());
            return;
        };
        let kind = if copy {
            TransferKind::Copy
        } else {
            TransferKind::Move
        };
        match TransferPlan::new(
            kind,
            &groups_by_folder(&paths),
            &Location::Path(destination),
        ) {
            // Dropping items back where they are is not a move.
            Ok(plan) if plan.is_noop() => {}
            Ok(plan) => {
                let _ = self.service.activate_panel(panel);
                self.submit_transfer(plan);
            }
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
        }
    }

    pub(super) fn operation_event(
        &mut self,
        event: OperationEvent,
        sender: &ComponentSender<Self>,
    ) {
        self.ui.panel.apply(&event);
        let op = &event.operation;
        if !op.is_finished() {
            return;
        }
        // The operation ended while its conflict dialog was still open.
        if let Some((id, window)) = &self.conflict_dialog
            && *id == op.id()
        {
            window.close();
        }
        self.conflict_queue.retain(|r| r.operation.id() != op.id());

        match (event.kind, op.kind()) {
            (OperationEventKind::Failed, _) if op.kind() != OperationKind::Trash => {
                self.notice = op.error.clone();
            }
            // Items the Trash refused were left alone; offer deletion.
            (_, OperationKind::Trash) if !op.failures.is_empty() => {
                let paths: Vec<Location> = op
                    .failures
                    .iter()
                    .map(|f| Location::Path(f.path.clone()))
                    .collect();
                let sender = sender.clone();
                dialogs::confirm(
                    &self.ui.window,
                    &format!(
                        "{} could not be moved to the Trash",
                        plural(paths.len(), "1 item", "items")
                    ),
                    "They were not deleted. The location may not support a Trash (for example some network or removable drives). Delete them permanently instead?",
                    "Delete Permanently",
                    move || {
                        sender.input_sender().emit(Msg::Submit(
                            OperationRequest::Delete {
                                items: paths.clone(),
                            },
                            None,
                        ));
                    },
                );
            }
            _ => {}
        }

        if self.hold.is_some() && self.operations.active_count() == 0 {
            tracing::info!("background operations finished; exiting");
            self.hold = None;
            relm4::main_application().quit();
        }
    }

    pub(super) fn show_next_conflict(&mut self, sender: &ComponentSender<Self>) {
        if self.conflict_dialog.is_some() {
            return;
        }
        let Some(request) = self.conflict_queue.pop_front() else {
            return;
        };
        // A hidden window with work left must come back to ask.
        if !gtk::prelude::WidgetExt::is_visible(&self.ui.window) {
            gtk::prelude::GtkWindowExt::present(&self.ui.window);
            self.hold = None;
        }
        let reply = request.reply;
        let sender = sender.clone();
        let window = dialogs::conflict(&self.ui.window, &request.conflict, move |decision| {
            let _ = reply.send(decision);
            sender.input_sender().emit(Msg::ConflictAnswered);
        });
        self.conflict_dialog = Some((request.operation.id(), window));
    }
}

/// Groups loose paths (clipboard, drag and drop) by their folder, so a
/// transfer can say where its items come from.
fn groups_by_folder(paths: &[PathBuf]) -> Vec<SelectionGroup> {
    let mut by_folder: BTreeMap<PathBuf, Vec<Location>> = BTreeMap::new();
    let mut order = Vec::new();
    for path in paths {
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if !by_folder.contains_key(&folder) {
            order.push(folder.clone());
        }
        by_folder
            .entry(folder)
            .or_default()
            .push(Location::Path(path.clone()));
    }
    order
        .into_iter()
        .map(|folder| SelectionGroup {
            // Not from a panel; the id only distinguishes groups.
            panel: PanelId::from_raw(0),
            items: by_folder.remove(&folder).unwrap_or_default(),
            folder: Location::Path(folder),
        })
        .collect()
}
