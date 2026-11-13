//! Keyboard intents and the command line. Both end in the same service
//! calls as the mouse, menus and toolbar.

use relm4::ComponentSender;
use seshat_application::interaction::command::{BranchCommand, COMMANDS, Command, Destination};
use seshat_application::interaction::{Action, AppAction, Intent};
use seshat_application::workspace::file_actions::ItemKind;
use seshat_application::workspace::{TransferKind, TransferPlan};
use seshat_domain::Direction;
use seshat_domain::entities::Location;
use seshat_domain::workspace::PruneMotion;

use super::command_line::CommandLineOutcome;
use super::{App, Msg, ThemeMsg};
use crate::dialogs;
use crate::notice::{self, Notice};
use crate::workspace_view::folder_title;

impl App {
    pub(super) fn intent(&mut self, intent: Intent, sender: &ComponentSender<Self>) {
        let active = self.service.active_panel();
        // A key press dismisses the last message.
        if !matches!(intent, Intent::Pending) {
            self.notice = None;
        }
        match intent {
            Intent::Pending | Intent::Cancelled | Intent::Unhandled => {}
            Intent::MoveCursor { delta, extend } => {
                self.service.move_cursor(active, delta);
                self.after_cursor_move(extend);
            }
            Intent::CursorFirst { extend } => {
                self.service.cursor_to_first(active);
                self.after_cursor_move(extend);
            }
            Intent::CursorLast { extend } => {
                self.service.cursor_to_last(active);
                self.after_cursor_move(extend);
            }
            Intent::HalfPage { down, extend } => {
                let page = self
                    .ui
                    .workspace
                    .with_panel(active, |v| v.files.rows_per_page())
                    .unwrap_or(10);
                let step = (page / 2).max(1) as isize;
                self.service
                    .move_cursor(active, if down { step } else { -step });
                self.after_cursor_move(extend);
            }
            Intent::ToggleSelection => {
                if self.service.cursor_index(active).is_none() {
                    self.service.cursor_to_first(active);
                }
                self.service.toggle_selection_at_cursor(active);
            }
            Intent::StartRange => {
                if self.service.cursor_index(active).is_none() {
                    self.service.cursor_to_first(active);
                }
                self.service.start_range(active);
            }
            Intent::EndRange | Intent::LeaveVisual => {}
            Intent::ClearSelection => {
                // Esc closes what is open first, then clears the selection.
                if !self.search.close() && !self.address.stop_editing() {
                    self.service.clear_selections();
                }
            }
            Intent::OpenCommandLine => self.open_command_line(),
            Intent::Open { launch_files } => self.open_cursor(launch_files),
            Intent::NotInThisMode(action) => self.notice = Some(notice::not_in_mode(action)),
            Intent::Perform(action) => self.perform(action.into(), sender),
        }
    }

    /// The single dispatcher: carries out a window action, whether it came
    /// from a key, a menu, the toolbar, a shortcut, a mouse button or the
    /// command line. Never add a second handler for one of these.
    pub(super) fn perform(&mut self, action: AppAction, sender: &ComponentSender<Self>) {
        let active = self.service.active_panel();
        match action {
            AppAction::Key(action) => self.perform_key(action, sender),
            AppAction::OpenSelection => self.open_targets(),
            AppAction::CopyPaths => self.copy_paths(),
            AppAction::Refresh => self.service.reload(active),
            AppAction::Rename => self.rename_selection(None, sender),
            AppAction::Trash => self.trash_selection(sender),
            AppAction::Delete => self.delete_selection(sender),
            AppAction::Restore => self.restore_selection(sender),
            AppAction::NewItem(item) => self.new_item(item, None, sender),
            AppAction::EmptyTrash => self.empty_trash(sender),
            AppAction::ToggleHidden => {
                self.service.toggle_hidden(active);
                self.save_preferences();
            }
            AppAction::SetViewMode(mode) => {
                self.service.set_view_mode(active, mode);
                self.save_preferences();
                self.focused_panel = None; // the visible view changed
            }
            AppAction::SetSortKey(key) => {
                self.service.set_sort_key(active, key);
                self.save_preferences();
            }
            AppAction::ToggleSortDescending => {
                self.service.toggle_sort_descending(active);
                self.save_preferences();
            }
            AppAction::ShowBranches => self.show_branches(),
            AppAction::CloseBranch => self.close_branch(None),
            AppAction::Help => self.show_help(),
            AppAction::ReloadThemes => self.theme_message(ThemeMsg::Reload, sender),
            AppAction::OpenThemesFolder => self.theme_message(ThemeMsg::OpenFolder, sender),
            AppAction::EditAddress => self.address.edit(),
            AppAction::KeyboardContextMenu => self.keyboard_context_menu(sender),
        }
    }

    /// Carries out a key-bindable action, the same in every mode.
    fn perform_key(&mut self, action: Action, sender: &ComponentSender<Self>) {
        match action {
            Action::PanelLeft => {
                self.service.move_left();
            }
            Action::PanelRight => {
                self.service.move_right();
            }
            Action::Parent => {
                self.service.go_parent();
            }
            Action::Open => {
                // Through the mode machine, which decides whether files may open.
                let intent = self.keys.borrow_mut().interaction.apply(action);
                self.intent(intent, sender);
            }
            Action::BranchPrevious => {
                self.service.cycle_branch(Direction::Previous);
            }
            Action::BranchNext => {
                self.service.cycle_branch(Direction::Next);
            }
            Action::Back => {
                self.service.go_back();
            }
            Action::Forward => {
                self.service.go_forward();
            }
            Action::Search => self.search.start(),
            Action::FocusMode => {
                self.service.toggle_focus_mode();
            }
            Action::Yank => self.clipboard_store(TransferKind::Copy),
            Action::Cut => self.clipboard_store(TransferKind::Move),
            Action::Paste => self.paste(sender),
            Action::PruneBack => self.prune_back(PruneMotion::Step),
            Action::PruneBackSegment => self.prune_back(PruneMotion::Segment),
            Action::UndoWorkspace => self.undo_workspace(),
            Action::FuzzyFind => self.open_fuzzy_find(sender),
            // Mode actions are resolved by the interaction layer.
            Action::CursorDown
            | Action::CursorUp
            | Action::CursorFirst
            | Action::CursorLast
            | Action::HalfPageDown
            | Action::HalfPageUp
            | Action::Visual
            | Action::VisualRange
            | Action::CommandLine
            | Action::Escape => {
                let intent = self.keys.borrow_mut().interaction.apply(action);
                self.intent(intent, sender);
            }
        }
    }

    /// `prune-back` / `prune-back-segment` (`db` / `dB` by default): the
    /// service resolves and prunes; this only reports. Workspace structure
    /// only: files are never touched.
    fn prune_back(&mut self, motion: PruneMotion) {
        let ws = self.service.workspace();
        let (branch, panel) = (ws.active_branch().id(), ws.active_panel());
        self.notice = Some(match self.service.prune_back(branch, panel, motion) {
            Ok(outcome) => {
                let undo = self
                    .keys
                    .borrow()
                    .keymap
                    .keys_for(Action::UndoWorkspace)
                    .first()
                    .cloned();
                notice::pruned(&outcome, undo.as_deref())
            }
            Err(err) => notice::not_pruned(&err),
        });
    }

    /// `undo-workspace` (`u` by default): undoes the last prune. Never undoes file operations.
    fn undo_workspace(&mut self) {
        self.notice = Some(match self.service.undo_workspace() {
            Ok(outcome) => notice::restored(outcome.restored),
            Err(err) => Notice::from_error(&err),
        });
    }

    fn after_cursor_move(&mut self, extend: bool) {
        let active = self.service.active_panel();
        if extend {
            self.service.extend_range(active);
        }
        let cursor = self.service.state(active).and_then(|s| s.cursor.clone());
        self.ui.workspace.focus_panel(active, cursor.as_ref());
    }

    /// `l` / `Enter`: a folder under the cursor opens through the
    /// workspace's context-aware resolution (continue, return to or start a
    /// branch). A file opens in its application, but not in Visual Mode
    /// (`launch_files` is false there, decision C6); with several files
    /// selected in the panel and the cursor among them, all open. With
    /// nothing under the cursor (an empty folder), it moves to the next
    /// panel of the branch, as `L` does (decision C2).
    fn open_cursor(&mut self, launch_files: bool) {
        let active = self.service.active_panel();
        let Some(entry) = self.service.cursor_entry(active).cloned() else {
            // Only this panel's selection: never open something selected
            // elsewhere just because this panel has no cursor.
            let selected_here = self
                .service
                .state(active)
                .is_some_and(|s| !s.selection.is_empty());
            if launch_files && selected_here {
                self.open_targets();
            } else {
                self.service.move_right();
            }
            return;
        };
        if !entry.is_browsable() && !launch_files {
            self.notice = Some(notice::files_not_in_visual());
            return;
        }
        let selected = self
            .service
            .state(active)
            .map(|s| s.selection.items().to_vec())
            .unwrap_or_default();
        if !entry.is_browsable() && selected.len() > 1 && selected.contains(&entry.location) {
            self.open_targets();
        } else {
            self.open_entries(active, vec![entry]);
        }
    }

    // Command line

    pub(super) fn open_command_line(&mut self) {
        self.command_line.open();
    }

    pub(super) fn close_command_line(&mut self) {
        self.keys.borrow_mut().interaction.leave_command_line();
        self.command_line.hide();
        self.focused_panel = None; // give the keyboard back to the panel
    }

    /// Carries out what the command line asks for after a message.
    pub(super) fn command_line_outcome(
        &mut self,
        outcome: CommandLineOutcome,
        sender: &ComponentSender<Self>,
    ) {
        match outcome {
            CommandLineOutcome::Stay => {}
            CommandLineOutcome::Close => self.close_command_line(),
            CommandLineOutcome::Run(parsed) => {
                self.close_command_line();
                match parsed {
                    Ok(command) => {
                        self.notice = None;
                        self.run_command(command, sender);
                    }
                    Err(err) => self.notice = Some(Notice::from_error(&err)),
                }
            }
        }
    }

    fn run_command(&mut self, command: Command, sender: &ComponentSender<Self>) {
        // A command with a window action equivalent goes through the same
        // dispatcher as keys, menus and the toolbar.
        if let Some(action) = command.as_action() {
            self.perform(action, sender);
            return;
        }
        let active = self.service.active_panel();
        match command {
            Command::Copy(destination) => {
                self.transfer_command(TransferKind::Copy, destination, sender)
            }
            Command::Move(destination) => {
                self.transfer_command(TransferKind::Move, destination, sender)
            }
            Command::Rename(name) => self.rename_selection(name, sender),
            Command::Mkdir(name) => self.new_item(ItemKind::Folder, Some(name), sender),
            Command::Touch(name) => self.new_item(ItemKind::File, Some(name), sender),
            Command::Search(query) => {
                self.service.set_query(active, &query);
                self.focused_panel = None;
            }
            // `:sort <key> asc|desc`: the two window actions it combines.
            Command::Sort { key, descending } => {
                self.perform(AppAction::SetSortKey(key), sender);
                if let Some(descending) = descending
                    && self
                        .service
                        .state(active)
                        .is_some_and(|s| s.options.sort.descending != descending)
                {
                    self.perform(AppAction::ToggleSortDescending, sender);
                }
            }
            Command::Branch(BranchCommand::Switch(n)) => {
                match self.service.workspace().branch_number(n) {
                    Some(branch) => {
                        let _ = self.service.activate_branch(branch);
                    }
                    None => self.notice = Some(notice::no_branch(n)),
                }
            }
            Command::Open(text) => match self.resolve_address(&text) {
                Ok(location) => self.service.open_location(location),
                Err(err) => self.notice = Some(Notice::from_error(&err)),
            },
            Command::Delete { .. }
            | Command::View(_)
            | Command::Hidden
            | Command::Help
            | Command::Focus
            | Command::Branch(
                BranchCommand::Next
                | BranchCommand::Previous
                | BranchCommand::Overview
                | BranchCommand::Close,
            ) => unreachable!("carried out above through `Command::as_action`"),
        }
    }

    /// `:copy` / `:move`: to a typed folder, a branch's active panel, or
    /// (without an argument) a folder picked from the open panels.
    fn transfer_command(
        &mut self,
        kind: TransferKind,
        destination: Option<Destination>,
        sender: &ComponentSender<Self>,
    ) {
        let targets = self.service.targets();
        if targets.is_empty() {
            self.notice = Some(notice::select_first());
            return;
        }
        let destination = match destination {
            Some(Destination::Branch(n)) => match self.service.workspace().branch_number(n) {
                Some(branch) => {
                    let panel = self
                        .service
                        .workspace()
                        .branch(branch)
                        .expect("listed")
                        .cursor();
                    self.service.location(panel).cloned()
                }
                None => {
                    self.notice = Some(notice::no_branch(n));
                    return;
                }
            },
            Some(Destination::Path(text)) => match self.resolve_address(&text) {
                Ok(location) => Some(location),
                Err(err) => {
                    self.notice = Some(Notice::from_error(&err));
                    return;
                }
            },
            None => None,
        };
        match destination {
            Some(destination) => self.plan_transfer(kind, &destination, sender),
            None => self.pick_destination(kind, sender),
        }
    }

    /// Plans a transfer of the targets and asks for confirmation when they
    /// come from several panels (decision D7).
    pub(super) fn plan_transfer(
        &mut self,
        kind: TransferKind,
        destination: &Location,
        sender: &ComponentSender<Self>,
    ) {
        match TransferPlan::new(kind, &self.service.targets(), destination) {
            Ok(plan) if plan.is_noop() => {
                self.notice = Some(notice::already_in_folder());
            }
            Ok(plan) => self.confirm_transfer(plan, sender),
            Err(err) => self.notice = Some(Notice::from_error(&err)),
        }
    }

    pub(super) fn confirm_transfer(&mut self, plan: TransferPlan, sender: &ComponentSender<Self>) {
        if !plan.spans_panels() {
            self.submit_transfer(plan);
            return;
        }
        let details = plan
            .origins
            .iter()
            .map(|(folder, n)| format!("• {n} from {}", crate::pathbar::address_text(folder)))
            .collect::<Vec<_>>()
            .join("\n");
        let title = format!("{}?", super::file_ops::transfer_summary(&plan));
        let verb = plan.kind.verb();
        let sender = sender.clone();
        dialogs::confirm_safe(
            &self.ui.window,
            &title,
            &format!("{details}\n\nInto {}", plan.destination.display()),
            verb,
            move || {
                sender
                    .input_sender()
                    .emit(Msg::SubmitTransfer(plan.clone()))
            },
        );
    }

    /// Submits a planned transfer. Its paths were captured when it was
    /// planned, so later navigation cannot change it. The selection is
    /// cleared once it is handed over; on failure it is kept.
    pub(super) fn submit_transfer(&mut self, plan: TransferPlan) {
        match self.operations.submit(plan.request()) {
            Ok(_) => {
                self.service.clear_selections();
                self.keys.borrow_mut().interaction.reset();
            }
            Err(err) => self.notice = Some(Notice::from_error(&err)),
        }
    }

    /// The destination picker: every open panel's folder, by branch.
    fn pick_destination(&mut self, kind: TransferKind, sender: &ComponentSender<Self>) {
        let ws = self.service.workspace();
        let mut options: Vec<(String, Location)> = Vec::new();
        for (i, branch) in ws.branches().iter().enumerate() {
            for panel in ws.path(branch.id()) {
                let location = ws.panel(panel).expect("path panel").location().clone();
                if location.is_trash() || options.iter().any(|(_, l)| *l == location) {
                    continue;
                }
                options.push((
                    format!(
                        "b{} · {} — {}",
                        i + 1,
                        folder_title(&location),
                        crate::pathbar::address_text(&location)
                    ),
                    location,
                ));
            }
        }
        let sender = sender.clone();
        dialogs::choose(
            &self.ui.window,
            &format!("{} to…", kind.verb()),
            &options,
            move |location| sender.input_sender().emit(Msg::TransferTo(kind, location)),
        );
    }

    pub(super) fn show_branches(&mut self) {
        let ws = self.service.workspace();
        let active = ws.active_branch().id();
        let mut lines = Vec::new();
        for (i, branch) in ws.branches().iter().enumerate() {
            let path: Vec<String> = ws
                .path(branch.id())
                .iter()
                .map(|&p| {
                    let name = folder_title(ws.panel(p).expect("path panel").location());
                    if p == branch.cursor() {
                        format!("[{name}]")
                    } else {
                        name
                    }
                })
                .collect();
            let marker = if branch.id() == active { "▶" } else { " " };
            lines.push(format!("{marker} {}  {}", i + 1, path.join(" › ")));
        }
        dialogs::info(
            &self.ui.window,
            "Branches",
            &format!(
                "{}\n\nSwitch with [ and ], :branch N, or the branch bar. Close with :branch close.",
                lines.join("\n")
            ),
        );
    }

    pub(super) fn show_help(&mut self) {
        let keys = self.keys.borrow();
        let mut text = String::from("Keys\n");
        for action in Action::ALL {
            let bound = keys.keymap.keys_for(action);
            if !bound.is_empty() {
                text.push_str(&format!(
                    "  {:<14} {}\n",
                    bound.join(" "),
                    action.description()
                ));
            }
        }
        text.push_str("\nCommands\n");
        for (name, description) in COMMANDS {
            text.push_str(&format!("  :{name:<9} {description}\n"));
        }
        drop(keys);
        dialogs::info(&self.ui.window, "Keys and Commands", &text);
    }
}
