//! File-operation commands. Every change goes through the
//! [`OperationManager`](seshat_application::operations::OperationManager).
//! What an action targets, and whether it may run, is decided by the
//! application layer ([`file_actions`]); the UI only asks for confirmation
//! or a name, shows results and submits the captured request.
//!
//! Sources are the workspace-wide selection (or the active panel's cursor
//! item), captured as absolute paths when the plan is made. A destination
//! is always explicit: the panel pasted or dropped into, or a folder named
//! on the command line. Nothing depends on which panel happens to be
//! active when an operation runs.

use std::path::{Path, PathBuf};

use relm4::ComponentSender;
use seshat_application::operations::OperationRequest;
use seshat_application::workspace::file_actions::{
    self, ActionPlan, ActionRefusal, Confirmation, NameKind,
};
use seshat_application::workspace::{TransferKind, TransferPlan};
use seshat_domain::entities::Location;
use seshat_domain::workspace::PanelId;

use super::{App, Msg, Staged};
use crate::clipboard::{self, ClipboardFiles};
use crate::dialogs;
use crate::notice::{self, Notice, count};

fn quoted_or_count(items: &[Location]) -> String {
    match items {
        [single] => format!("“{}”", crate::workspace_view::folder_title(single)),
        many => count(many.len(), "item", "items"),
    }
}

/// "Copy 5 items from 2 folders to “Downloads”": the heading of a transfer
/// confirmation.
pub(super) fn transfer_summary(plan: &TransferPlan) -> String {
    let items = notice::count(plan.sources.len(), "item", "items");
    let from = match plan.origins.len() {
        1 => String::new(),
        n => format!(" from {n} folders"),
    };
    let to = plan
        .destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| plan.destination.display().to_string());
    format!("{} {items}{from} to “{to}”", plan.kind.verb())
}

/// "• 2 from ~/Documents" lines for a confirmation.
fn origin_lines(origins: &[(Location, usize)]) -> String {
    origins
        .iter()
        .map(|(folder, n)| format!("• {n} from {}", crate::pathbar::address_text(folder)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Heading, body and button of a confirmation dialog.
fn confirmation_text(confirmation: &Confirmation) -> (String, String, &'static str) {
    const UNRECOVERABLE: &str = "Deleted items are not moved to the Trash and cannot be recovered.";
    match confirmation {
        Confirmation::TrashAcrossPanels { count, origins } => (
            format!(
                "Move {} to the Trash?",
                notice::count(*count, "item", "items")
            ),
            format!(
                "The items come from several panels:\n{}",
                origin_lines(origins)
            ),
            "Move to Trash",
        ),
        Confirmation::Delete { items, origins } => (
            format!("Permanently delete {}?", quoted_or_count(items)),
            match origins {
                Some(origins) => format!(
                    "The items come from several panels:\n{}\n\n{UNRECOVERABLE}",
                    origin_lines(origins)
                ),
                None => UNRECOVERABLE.to_owned(),
            },
            "Delete Permanently",
        ),
        Confirmation::EmptyTrash { count } => (
            "Empty the Trash?".to_owned(),
            format!(
                "{} will be permanently deleted. This cannot be undone.",
                notice::count(*count, "item", "items")
            ),
            "Empty Trash",
        ),
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
            Err(err) => self.notice = Some(Notice::from_error(&err)),
        }
    }

    /// Says why an action did not run, unless the user can already see it.
    fn refuse(&mut self, refusal: &ActionRefusal) {
        if !refusal.is_silent() {
            self.notice = Some(Notice::from_error(refusal));
        }
    }

    /// Carries out a file action's plan: submit it, or first ask for
    /// confirmation or a name. The request was captured when the plan was
    /// made, so nothing is looked up again when the user answers.
    pub(super) fn carry_out(
        &mut self,
        plan: Result<ActionPlan, ActionRefusal>,
        sender: &ComponentSender<Self>,
    ) {
        let plan = match plan {
            Ok(plan) => plan,
            Err(refusal) => return self.refuse(&refusal),
        };
        let sender = sender.clone();
        match plan {
            ActionPlan::Submit { request, select } => self.submit(request, select),
            ActionPlan::Confirm {
                request,
                confirmation,
            } => {
                let (heading, body, button) = confirmation_text(&confirmation);
                dialogs::confirm(&self.ui.window, &heading, &body, button, move || {
                    sender
                        .input_sender()
                        .emit(Msg::Submit(request.clone(), None))
                });
            }
            ActionPlan::AskName(prompt) => {
                let (title, button) = match prompt.kind {
                    NameKind::Rename => ("Rename", "Rename"),
                    NameKind::NewFolder => ("New Folder", "Create"),
                    NameKind::NewFile => ("New File", "Create"),
                };
                let suggestion = prompt.suggestion.clone();
                dialogs::name_prompt(
                    &self.ui.window,
                    title,
                    button,
                    &suggestion,
                    prompt.select_stem,
                    move |name| {
                        let (request, select) = prompt.complete(name);
                        sender.input_sender().emit(Msg::Submit(request, select));
                    },
                );
            }
        }
    }

    /// Copy or cut: the targets go to the system clipboard, interoperable
    /// with other file managers (decision D9). The selection is consumed.
    pub(super) fn clipboard_store(&mut self, cut: bool) {
        let sources = match file_actions::clipboard_sources(&self.service) {
            Ok(sources) => sources,
            Err(refusal) => return self.refuse(&refusal),
        };
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let paths = sources.value;
        clipboard::set_files(&display, &paths, cut);
        self.staged = Some(Staged {
            cut,
            count: paths.len(),
        });
        self.notice = notice::left_out(sources.left_out);
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
            self.notice = Some(Notice::error("The clipboard has no files to paste"));
            return;
        };
        let Some(destination) = self.active_folder() else {
            self.notice = Some(Notice::error("Files can't be pasted here"));
            return;
        };
        let kind = if files.cut {
            TransferKind::Move
        } else {
            TransferKind::Copy
        };
        let plan = TransferPlan::from_paths(kind, &files.paths, &Location::Path(destination));
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
            Err(err) => self.notice = Some(Notice::from_error(&err)),
        }
    }

    pub(super) fn rename_selection(
        &mut self,
        name: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let plan = file_actions::rename(&self.service, name);
        self.carry_out(plan, sender);
    }

    /// Moves the targets to the Trash. When they come from several panels,
    /// the user confirms first (decision D7).
    pub(super) fn trash_selection(&mut self, sender: &ComponentSender<Self>) {
        match file_actions::trash(&self.service) {
            Ok(plan) => {
                // Set first, so a submission error from `carry_out` wins.
                self.notice = notice::left_out(plan.left_out);
                self.carry_out(Ok(plan.value), sender);
            }
            Err(refusal) => self.refuse(&refusal),
        }
    }

    pub(super) fn delete_selection(&mut self, sender: &ComponentSender<Self>) {
        let plan = file_actions::delete(&self.service);
        self.carry_out(plan, sender);
    }

    pub(super) fn restore_selection(&mut self, sender: &ComponentSender<Self>) {
        let plan = file_actions::restore(&self.service);
        self.carry_out(plan, sender);
    }

    /// Creates a folder or file in the active panel. With `name` (from the
    /// command line) it is created directly; otherwise a name is asked for.
    pub(super) fn new_item(
        &mut self,
        directory: bool,
        name: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let plan = file_actions::new_item(&self.service, directory, name);
        self.carry_out(plan, sender);
    }

    pub(super) fn empty_trash(&mut self, sender: &ComponentSender<Self>) {
        let plan = file_actions::empty_trash(&self.service);
        self.carry_out(plan, sender);
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
            self.notice = Some(Notice::error("Files can't be dropped here"));
            return;
        };
        let kind = if copy {
            TransferKind::Copy
        } else {
            TransferKind::Move
        };
        match TransferPlan::from_paths(kind, &paths, &Location::Path(destination)) {
            // Dropping items back where they are is not a move.
            Ok(plan) if plan.is_noop() => {}
            Ok(plan) => {
                let _ = self.service.activate_panel(panel);
                self.submit_transfer(plan);
            }
            Err(err) => self.notice = Some(Notice::from_error(&err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use seshat_application::workspace::{TransferKind, TransferPlan};
    use seshat_domain::entities::Location;

    use super::transfer_summary;

    fn plan(kind: TransferKind, sources: &[&str], destination: &str) -> TransferPlan {
        let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
        TransferPlan::from_paths(kind, &sources, &Location::Path(destination.into())).unwrap()
    }

    #[test]
    fn transfer_summaries_say_what_goes_where() {
        assert_eq!(
            transfer_summary(&plan(
                TransferKind::Copy,
                &["/h/Documents/a", "/h/Documents/b", "/h/Pictures/c"],
                "/h/Downloads"
            )),
            "Copy 3 items from 2 folders to “Downloads”"
        );
        assert_eq!(
            transfer_summary(&plan(TransferKind::Move, &["/a/x", "/a/y"], "/a")),
            "Move 2 items to “a”"
        );
        assert_eq!(
            transfer_summary(&plan(TransferKind::Copy, &["/a/x"], "/d")),
            "Copy 1 item to “d”"
        );
    }
}
