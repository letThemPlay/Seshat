//! The shell around the workspace service: carrying out its effects,
//! rendering it, and routing panel and branch events into it.

use std::time::Instant;

use gtk::prelude::*;
use relm4::{ComponentController, ComponentSender};
use seshat_application::ports::DirectoryChange;
use seshat_domain::entities::Location;
use seshat_domain::workspace::{BranchId, PanelId, ScrollAnchor};

use super::effects::reload_delay;
use super::{App, Msg};
use crate::files::FileViewEvent;
use crate::notice::{self, Notice};
use crate::panel_view::PanelEvent;
use crate::sidebar::SidebarInput;
use crate::workspace_view::WorkspaceEvent;

impl App {
    /// Brings the screen up to date with the service: runs pending
    /// effects, renders the workspace and moves keyboard focus to the
    /// active panel when it changed.
    pub(super) fn sync(&mut self, sender: &ComponentSender<Self>) {
        self.run_effects(sender);
        let started = Instant::now();
        let outcome = self.ui.workspace.render(&self.service);
        for (panel, offset) in outcome.dropped {
            self.remember_scroll(panel, offset);
        }
        if outcome.refocus {
            // The active panel moved in the layout and lost keyboard focus.
            self.focused_panel = None;
        }
        let full_views = outcome.visible.len();
        self.service.set_visible(&outcome.visible);
        self.run_effects(sender);
        // Timings for performance runs (docs/review/performance.md).
        tracing::debug!(
            elapsed_us = started.elapsed().as_micros() as u64,
            full_views,
            "workspace rendered"
        );

        let active = self.service.active_panel();
        let refocus =
            self.focused_panel != Some(active) || (!self.focused_on_files && self.active_ready());
        if refocus && !self.typing() {
            self.focus_active();
            self.sync_search();
            self.sidebar.emit(SidebarInput::SetCurrent(Some(
                self.service.active_location().clone(),
            )));
        }

        // One line per structural change, for diagnostics and the UI tests.
        // Locations only; never listings.
        let ws = self.service.workspace();
        let shape = (active, ws.branch_count(), ws.panel_count(), ws.focus_mode());
        if self.logged_shape != Some(shape) {
            self.logged_shape = Some(shape);
            tracing::debug!(
                panel = %active,
                location = ?self.service.active_location(),
                branch = ws.active_branch().id().raw(),
                branches = shape.1,
                panels = shape.2,
                focus_mode = shape.3,
                "workspace state"
            );
        }
    }

    /// Hands the service's pending effects to the runner.
    pub(super) fn run_effects(&mut self, sender: &ComponentSender<Self>) {
        let effects = self.service.take_effects();
        self.effects.run(effects, sender);
    }

    /// Saves the workspace now, with every shown panel's scroll position.
    pub(super) fn save_workspace(&mut self) {
        for (panel, offset) in self.ui.workspace.scroll_offsets() {
            self.remember_scroll(panel, offset);
        }
        self.effects.save(self.service.workspace());
    }

    /// Records a panel's scroll offset, anchored to its cursor (or first
    /// item) so a stale offset is dropped when that item disappears.
    fn remember_scroll(&mut self, panel: PanelId, offset: i32) {
        let item = self
            .service
            .state(panel)
            .and_then(|s| s.cursor.clone())
            .or_else(|| {
                self.service
                    .listing(panel)
                    .and_then(|l| l.shown().first())
                    .map(|e| e.location.clone())
            });
        let anchor = item.map(|item| ScrollAnchor { item, offset });
        self.service.set_scroll(panel, anchor);
    }

    pub(super) fn workspace_event(
        &mut self,
        event: WorkspaceEvent,
        sender: &ComponentSender<Self>,
    ) {
        // A pointer action changes the context a pending key sequence (such
        // as the `d` of `db`) was typed in: drop it. Resizes and selection
        // updates are not user actions on the workspace.
        let pointer = !matches!(
            event,
            WorkspaceEvent::Resized
                | WorkspaceEvent::Panel(_, PanelEvent::Files(FileViewEvent::SelectionChanged))
        );
        if pointer {
            self.keys.borrow_mut().interaction.cancel_pending();
        }
        match event {
            WorkspaceEvent::Panel(panel, event) => self.panel_event(panel, event, sender),
            WorkspaceEvent::ActivateBranch(branch) => {
                let _ = self.service.activate_branch(branch);
            }
            WorkspaceEvent::CloseBranch(branch) => self.close_branch(Some(branch)),
            WorkspaceEvent::ShowBranches => self.show_branches(),
            WorkspaceEvent::Resized => {}
        }
    }

    fn panel_event(&mut self, panel: PanelId, event: PanelEvent, sender: &ComponentSender<Self>) {
        match event {
            PanelEvent::Activate | PanelEvent::Neighbour(_) | PanelEvent::OpenChild(_) => {
                let target = match event {
                    PanelEvent::Neighbour(p) | PanelEvent::OpenChild(p) => p,
                    _ => panel,
                };
                if self.service.active_panel() != target {
                    let _ = self.service.activate_panel(target);
                }
            }
            PanelEvent::Retry => self.service.reload(panel),
            PanelEvent::Parent => {
                let ancestor = self.service.workspace().nearest_available_ancestor(panel);
                match ancestor {
                    Some(ancestor) => {
                        let _ = self.service.activate_panel(ancestor);
                    }
                    None => self
                        .service
                        .open_location(Location::Path(self.home.clone())),
                }
            }
            PanelEvent::CloseBranch => {
                let _ = self.service.activate_panel(panel);
                self.close_branch(None);
            }
            PanelEvent::Files(event) => self.files_event(panel, event, sender),
        }
    }

    fn files_event(
        &mut self,
        panel: PanelId,
        event: FileViewEvent,
        sender: &ComponentSender<Self>,
    ) {
        match event {
            FileViewEvent::Activate(position) => {
                let entry = self
                    .ui
                    .workspace
                    .with_panel(panel, |v| v.files.entry(position))
                    .flatten();
                if let Some(entry) = entry {
                    self.service.set_cursor(panel, Some(entry.location.clone()));
                    let selected = self
                        .service
                        .state(panel)
                        .map(|s| s.selection.items().to_vec())
                        .unwrap_or_default();
                    // Activating one of several selected files opens them all.
                    if !entry.is_browsable()
                        && selected.len() > 1
                        && selected.contains(&entry.location)
                    {
                        let _ = self.service.activate_panel(panel);
                        self.open_targets();
                    } else {
                        self.open_entries(panel, vec![entry]);
                    }
                }
            }
            FileViewEvent::ContextMenu { position, x, y } => {
                sender.input_sender().emit(Msg::ContextMenu {
                    panel,
                    on_item: position.is_some(),
                    x,
                    y,
                });
            }
            FileViewEvent::SelectionChanged => {
                let items = self
                    .ui
                    .workspace
                    .with_panel(panel, |v| v.files.selected_locations())
                    .unwrap_or_default();
                self.service.set_selection(panel, items);
            }
            FileViewEvent::Drop { paths, into, copy } => {
                self.drop_files(panel, paths, into, copy);
            }
        }
    }

    /// Keyboard focus moved inside a file list: that panel becomes active
    /// and the focused row becomes its cursor.
    pub(super) fn focus_moved(&mut self) {
        let Some(focus) = gtk::prelude::GtkWindowExt::focus(&self.ui.window) else {
            return;
        };
        let Some((panel, row)) = self.ui.workspace.panel_at(&focus) else {
            return;
        };
        if self.service.active_panel() != panel {
            // Focus moved to another panel: a pending sequence must not
            // complete there.
            self.keys.borrow_mut().interaction.cancel_pending();
            let _ = self.service.activate_panel(panel);
        }
        self.focused_panel = Some(panel);
        self.focused_on_files = true;
        if let Some(row) = row {
            self.service.set_cursor(panel, Some(row));
            if matches!(
                self.keys.borrow().mode(),
                seshat_application::interaction::Mode::Visual { range: true }
            ) {
                self.service.extend_range(panel);
            }
        }
    }

    pub(super) fn panel_changed(
        &mut self,
        panel: PanelId,
        generation: u64,
        change: DirectoryChange,
        sender: &ComponentSender<Self>,
    ) {
        let reload = self.service.directory_changed(panel, generation, change);
        if change == DirectoryChange::Removed && self.service.active_panel() == panel {
            self.notice = Some(notice::folder_removed());
        }
        if reload {
            // Large folders wait longer, so constant changes can't keep the
            // UI busy.
            let entries = self
                .service
                .listing(self.service.active_panel())
                .map_or(0, |l| l.total());
            self.effects
                .schedule_reload(panel, generation, reload_delay(entries), sender);
        }
    }

    /// Closes `branch`, or the active branch. Files are never touched, and
    /// running operations keep going (they hold their own paths).
    pub(super) fn close_branch(&mut self, branch: Option<BranchId>) {
        let branch = branch.unwrap_or_else(|| self.service.workspace().active_branch().id());
        match self.service.close_branch(branch) {
            Ok(removed) => {
                for panel in removed {
                    self.effects.reload_done(panel);
                }
                self.focused_panel = None;
            }
            Err(err) => self.notice = Some(Notice::from_error(&err)),
        }
    }

    /// `/` or Ctrl+F. Like Vim's `/`, this starts a new search: a panel's
    /// remembered query (restored with the panel) is selected, so typing
    /// replaces it and an arrow key keeps it for editing.
    pub(super) fn start_search(&mut self) {
        self.ui.search_bar.set_search_mode(true);
        self.ui.search_entry.grab_focus();
        self.ui.search_entry.select_region(0, -1);
    }

    /// Shows the active panel's own search in the search bar.
    fn sync_search(&mut self) {
        let query = self
            .service
            .state(self.service.active_panel())
            .map(|s| s.options.query.clone())
            .unwrap_or_default();
        if self.ui.search_entry.text() != query {
            self.search_echo = Some(query.clone());
            self.ui.search_entry.set_text(&query);
        }
        let show = !query.is_empty();
        if self.ui.search_bar.is_search_mode() != show {
            if !show {
                self.search_echo = Some(String::new());
            }
            self.ui.search_bar.set_search_mode(show);
        }
    }
}
