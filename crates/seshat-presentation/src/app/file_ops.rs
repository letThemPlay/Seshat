//! File-operation commands. Every change goes through the
//! [`OperationManager`](seshat_application::operations::OperationManager);
//! the UI only builds requests, asks for confirmation and shows results.

use std::path::{Path, PathBuf};

use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::operations::{
    OperationEvent, OperationEventKind, OperationRequest, unique_path,
};
use seshat_domain::entities::{FileEntry, Location};
use seshat_domain::operation::OperationKind;

use super::{App, Msg};
use crate::clipboard::{self, ClipboardFiles};
use crate::dialogs;

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_owned()
    } else {
        format!("{n} {many}")
    }
}

fn quoted_or_count(entries: &[FileEntry]) -> String {
    match entries {
        [single] => format!("“{}”", single.display_name),
        many => format!("{} items", many.len()),
    }
}

impl App {
    pub(super) fn in_trash(&self) -> bool {
        self.history.current().is_some_and(Location::is_trash)
    }

    /// The shown folder, if it is a regular folder.
    fn current_folder(&self) -> Option<PathBuf> {
        match self.history.current() {
            Some(Location::Path(path)) => Some(path.clone()),
            _ => None,
        }
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        self.ui
            .file_view
            .selected()
            .iter()
            .filter_map(|e| e.local_path().map(Path::to_path_buf))
            .collect()
    }

    pub(super) fn submit(&mut self, request: OperationRequest, select: Option<Location>) {
        match self.operations.submit(request) {
            Ok(_) => {
                if select.is_some() {
                    self.pending_select = select;
                }
            }
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
        }
    }

    pub(super) fn clipboard_store(&mut self, cut: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        clipboard::set_files(&display, &paths, cut);
        let what = plural(paths.len(), "1 item", "items");
        self.notice = Some(if cut {
            format!("Cut {what}. Paste to move.")
        } else {
            format!("Copied {what}.")
        });
    }

    pub(super) fn paste(&mut self, sender: &ComponentSender<Self>) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let sender = sender.clone();
        relm4::spawn_local(async move {
            let files = clipboard::read_files(&display).await;
            sender.input_sender().emit(Msg::PasteFiles(files));
        });
    }

    pub(super) fn paste_files(
        &mut self,
        files: Option<ClipboardFiles>,
        _sender: &ComponentSender<Self>,
    ) {
        let Some(files) = files else {
            self.notice = Some("The clipboard has no files to paste.".to_owned());
            return;
        };
        let Some(destination) = self.current_folder() else {
            self.notice = Some("Files can't be pasted here.".to_owned());
            return;
        };
        let request = if files.cut {
            OperationRequest::Move {
                sources: files.paths,
                destination,
            }
        } else {
            OperationRequest::Copy {
                sources: files.paths,
                destination,
            }
        };
        self.submit(request, None);
        // A cut is consumed by pasting it once.
        if files.cut
            && let Some(display) = gtk::gdk::Display::default()
        {
            clipboard::clear(&display);
        }
    }

    pub(super) fn rename_selection(&mut self, sender: &ComponentSender<Self>) {
        let selected = self.ui.file_view.selected();
        let [entry] = selected.as_slice() else {
            return;
        };
        let Some(path) = entry.local_path().map(Path::to_path_buf) else {
            return;
        };
        let sender = sender.clone();
        dialogs::name_prompt(
            &self.ui.window,
            "Rename",
            "Rename",
            &entry.display_name,
            !entry.is_directory(),
            move |new_name| {
                let select = path.parent().map(|p| Location::Path(p.join(&new_name)));
                let request = OperationRequest::Rename {
                    path: path.clone(),
                    new_name,
                };
                sender.input_sender().emit(Msg::Submit(request, select));
            },
        );
    }

    pub(super) fn trash_selection(&mut self, _sender: &ComponentSender<Self>) {
        let paths = self.selected_paths();
        if !paths.is_empty() {
            self.submit(OperationRequest::Trash { paths }, None);
        }
    }

    pub(super) fn delete_selection(&mut self, sender: &ComponentSender<Self>) {
        let entries = self.ui.file_view.selected();
        if entries.is_empty() {
            return;
        }
        let items: Vec<Location> = entries.iter().map(|e| e.location.clone()).collect();
        let sender = sender.clone();
        dialogs::confirm(
            &self.ui.window,
            &format!("Permanently delete {}?", quoted_or_count(&entries)),
            "Deleted items are not moved to the Trash and cannot be recovered.",
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

    pub(super) fn restore_selection(&mut self, _sender: &ComponentSender<Self>) {
        let items: Vec<PathBuf> = self
            .ui
            .file_view
            .selected()
            .into_iter()
            .filter_map(|e| match e.location {
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

    pub(super) fn new_item(&mut self, directory: bool, sender: &ComponentSender<Self>) {
        let Some(parent) = self.current_folder() else {
            return;
        };
        let base = if directory { "New Folder" } else { "New File" };
        // Suggest a free name based on what is already listed.
        let taken = |p: &Path| self.listing.iter().any(|e| e.local_path() == Some(p));
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
                let select = Some(Location::Path(parent.join(&name)));
                let request = if directory {
                    OperationRequest::CreateDirectory {
                        parent: parent.clone(),
                        name,
                    }
                } else {
                    OperationRequest::CreateFile {
                        parent: parent.clone(),
                        name,
                    }
                };
                sender.input_sender().emit(Msg::Submit(request, select));
            },
        );
    }

    pub(super) fn empty_trash(&mut self, sender: &ComponentSender<Self>) {
        if self.history.current() != Some(&Location::trash_root()) {
            self.notice = Some("Open the Trash to empty it.".to_owned());
            return;
        }
        let items: Vec<Location> = self.listing.iter().map(|e| e.location.clone()).collect();
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

    pub(super) fn drop_files(
        &mut self,
        paths: Vec<PathBuf>,
        into: Option<u32>,
        copy: bool,
        _sender: &ComponentSender<Self>,
    ) {
        let destination = into
            .and_then(|position| self.ui.file_view.entry(position))
            .and_then(|e| e.local_path().map(Path::to_path_buf))
            .or_else(|| self.current_folder());
        let Some(destination) = destination else {
            self.notice = Some("Files can't be dropped here.".to_owned());
            return;
        };
        // Dropping items back where they are is not a move.
        if !copy
            && paths
                .iter()
                .all(|p| p.parent() == Some(destination.as_path()))
        {
            return;
        }
        let request = if copy {
            OperationRequest::Copy {
                sources: paths,
                destination,
            }
        } else {
            OperationRequest::Move {
                sources: paths,
                destination,
            }
        };
        self.submit(request, None);
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
        if !self.ui.window.is_visible() {
            self.ui.window.present();
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

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
