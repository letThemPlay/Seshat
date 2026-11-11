//! File operations as the window sees them: following the operation
//! manager's events in the operations panel, asking about conflicts one
//! dialog at a time, and keeping the app alive while the window is hidden
//! with work left. The manager stays the single source of truth; this
//! only observes it and forwards requests.

use std::collections::VecDeque;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::operations::{
    OperationEvent, OperationEventKind, OperationManager, OperationRequest, RequestError,
};
use seshat_domain::entities::Location;
use seshat_domain::operation::{OperationId, OperationKind};

use super::{App, Cx, Msg};
use crate::conflicts::{ConflictRequest, UiConflictResolver};
use crate::dialogs;
use crate::notice::{Notice, count};
use crate::operations_panel::OperationsPanel;

#[derive(Debug)]
pub enum OperationsMsg {
    /// An event from the operation manager.
    Event(OperationEvent),
    Cancel(OperationId),
    /// A worker asks how to resolve a conflict.
    Conflict(ConflictRequest),
    ConflictAnswered,
    /// The window was closed while operations were running.
    WindowHidden,
}

pub(super) struct OperationsController {
    manager: OperationManager,
    panel: OperationsPanel,
    window: gtk::Window,
    /// Conflicts waiting for the open dialog to close.
    conflict_queue: VecDeque<ConflictRequest>,
    conflict_dialog: Option<(OperationId, gtk::Window)>,
    /// Keeps the process alive while the window is hidden with work left.
    hold: Option<gtk::gio::ApplicationHoldGuard>,
}

impl OperationsController {
    pub(super) fn new(
        manager: OperationManager,
        panel: OperationsPanel,
        window: gtk::Window,
    ) -> Self {
        Self {
            manager,
            panel,
            window,
            conflict_queue: VecDeque::new(),
            conflict_dialog: None,
            hold: None,
        }
    }

    /// Asks the user about conflicts and follows the manager's events.
    pub(super) fn connect(&self, sender: &ComponentSender<App>) {
        self.manager
            .set_conflict_resolver(Arc::new(UiConflictResolver::new(
                sender.input_sender().clone(),
            )));
        // Called on worker threads: only forward.
        let input = sender.input_sender().clone();
        self.manager.subscribe(Box::new(move |event| {
            let _ = input.send(Msg::Operations(OperationsMsg::Event(event.clone())));
        }));
    }

    /// Hands a request to the operation manager, which validates it.
    pub(super) fn submit(&self, request: OperationRequest) -> Result<OperationId, RequestError> {
        self.manager.submit(request)
    }

    pub(super) fn update(&mut self, msg: OperationsMsg, cx: &mut Cx<'_>) {
        match msg {
            OperationsMsg::Event(event) => self.event(event, cx),
            OperationsMsg::Cancel(id) => {
                let _ = self.manager.cancel(id);
            }
            OperationsMsg::Conflict(request) => {
                self.conflict_queue.push_back(request);
                self.show_next_conflict(cx.sender);
            }
            OperationsMsg::ConflictAnswered => {
                self.conflict_dialog = None;
                self.show_next_conflict(cx.sender);
            }
            OperationsMsg::WindowHidden => {
                tracing::info!(
                    active = self.manager.active_count(),
                    "window closed; finishing operations in the background"
                );
                self.hold = Some(relm4::main_application().hold());
            }
        }
    }

    fn event(&mut self, event: OperationEvent, cx: &mut Cx<'_>) {
        self.panel.apply(&event);
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
                *cx.notice = op.error.clone().map(Notice::error);
            }
            // Items the Trash refused were left alone; offer deletion.
            (_, OperationKind::Trash) if !op.failures.is_empty() => {
                let paths: Vec<Location> = op
                    .failures
                    .iter()
                    .map(|f| Location::Path(f.path.clone()))
                    .collect();
                let sender = cx.sender.clone();
                dialogs::confirm(
                    &self.window,
                    &format!(
                        "{} could not be moved to the Trash",
                        count(paths.len(), "item", "items")
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

        if self.hold.is_some() && self.manager.active_count() == 0 {
            tracing::info!("background operations finished; exiting");
            self.hold = None;
            relm4::main_application().quit();
        }
    }

    fn show_next_conflict(&mut self, sender: &ComponentSender<App>) {
        if self.conflict_dialog.is_some() {
            return;
        }
        let Some(request) = self.conflict_queue.pop_front() else {
            return;
        };
        // A hidden window with work left must come back to ask.
        if !self.window.is_visible() {
            self.window.present();
            self.hold = None;
        }
        let reply = request.reply;
        let sender = sender.clone();
        let window = dialogs::conflict(&self.window, &request.conflict, move |decision| {
            let _ = reply.send(decision);
            sender
                .input_sender()
                .emit(Msg::Operations(OperationsMsg::ConflictAnswered));
        });
        self.conflict_dialog = Some((request.operation.id(), window));
    }
}
