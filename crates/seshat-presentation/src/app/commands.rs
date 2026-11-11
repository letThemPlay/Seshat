//! Keyboard intents and the command line. Both end in the same service
//! calls as the mouse, menus and toolbar.

use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::interaction::command::{
    self, BranchCommand, COMMANDS, Command, Destination,
};
use seshat_application::interaction::completion::{self, CompletionCycle, Plan};
use seshat_application::interaction::{Action, Intent};
use seshat_application::navigation::parse_address;
use seshat_application::ports::ListingError;
use seshat_application::workspace::{PruneError, TransferKind, TransferPlan};
use seshat_domain::entities::Location;
use seshat_domain::workspace::{BranchId, PruneMotion};

use super::workspace::capitalise;
use super::{App, Msg};
use crate::dialogs;
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
                if self.ui.search_bar.is_search_mode() {
                    self.ui.search_bar.set_search_mode(false);
                } else if self.editing_address {
                    self.editing_address = false;
                } else {
                    self.service.clear_selections();
                }
            }
            Intent::OpenCommandLine => self.open_command_line(),
            Intent::Open { launch_files } => self.open_cursor(launch_files),
            Intent::NotInThisMode(_) => {
                self.notice =
                    Some("Pruning isn't available in Visual Mode; press Esc first.".to_owned());
            }
            Intent::Perform(action) => self.perform(action, sender),
        }
    }

    /// Carries out an action that is the same in every mode.
    pub(super) fn perform(&mut self, action: Action, sender: &ComponentSender<Self>) {
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
                self.service.cycle_branch(false);
            }
            Action::BranchNext => {
                self.service.cycle_branch(true);
            }
            Action::Back => {
                self.service.go_back();
            }
            Action::Forward => {
                self.service.go_forward();
            }
            Action::Search => self.start_search(),
            Action::FocusMode => {
                self.service.toggle_focus_mode();
            }
            Action::Yank => self.clipboard_store(false),
            Action::Cut => self.clipboard_store(true),
            Action::Paste => self.paste(sender),
            Action::PruneBack => self.prune_back(PruneMotion::Step),
            Action::PruneBackSegment => self.prune_back(PruneMotion::Segment),
            Action::UndoWorkspace => self.undo_workspace(),
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

    /// `db` / `dB`: the service resolves and prunes; this only reports.
    /// Workspace structure only: files are never touched.
    fn prune_back(&mut self, motion: PruneMotion) {
        let ws = self.service.workspace();
        let (branch, panel) = (ws.active_branch().id(), ws.active_panel());
        self.notice = Some(match self.service.prune_back(branch, panel, motion) {
            Ok(outcome) => {
                let mut text = match outcome.removed {
                    1 => "Pruned 1 panel".to_owned(),
                    n => format!("Pruned {n} panels"),
                };
                if outcome.ended_branch {
                    text.push_str("; the branch ended");
                }
                match outcome.deselected {
                    0 => {}
                    1 => text.push_str("; 1 selected item was deselected"),
                    n => text.push_str(&format!("; {n} selected items were deselected")),
                }
                if let Some(undo) = self
                    .keys
                    .borrow()
                    .keymap
                    .keys_for(Action::UndoWorkspace)
                    .first()
                {
                    text.push_str(&format!(". Press {undo} to undo"));
                }
                text.push('.');
                text
            }
            Err(PruneError::FocusMode) => "Pruning isn't available in Focus Mode.".to_owned(),
            Err(PruneError::Refused(refusal)) => format!("Nothing pruned: {refusal}."),
        });
    }

    /// `u`: undoes the last prune. Never undoes file operations.
    fn undo_workspace(&mut self) {
        self.notice = Some(match self.service.undo_workspace() {
            Ok(outcome) => match outcome.restored {
                1 => "Restored 1 panel.".to_owned(),
                n => format!("Restored {n} panels."),
            },
            Err(err) => format!("{}.", capitalise(&err.to_string())),
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

    /// Enter: open the cursor item; with several files selected in the
    /// panel and the cursor among them, open them all.
    /// `l` and `Enter` (the same action). A folder under the cursor opens
    /// through the workspace's context-aware resolution (continue, return
    /// to or start a branch). A file opens in its application, but not in
    /// Visual Mode (`launch_files` is false there, decision C6). With
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
            self.notice = Some("Files don't open in Visual Mode; press Esc first.".to_owned());
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

    // ── Command line ────────────────────────────────────────────────────

    /// Opens the command line. From the keyboard, the key controller has
    /// already opened and focused it (so no typed key is lost); this only
    /// covers other ways in.
    pub(super) fn open_command_line(&mut self) {
        if WidgetExt::is_visible(&self.ui.command_entry) {
            return;
        }
        self.ui.command_entry.set_text("");
        self.ui.command_entry.set_visible(true);
        self.ui.command_entry.grab_focus();
    }

    pub(super) fn close_command_line(&mut self) {
        self.keys.borrow_mut().interaction.leave_command_line();
        self.ui.command_entry.set_visible(false);
        self.focused_panel = None; // give the keyboard back to the panel
    }

    pub(super) fn command_history_step(&mut self, older: bool) {
        let text = if older {
            self.command_history.older().map(str::to_owned)
        } else {
            self.command_history.newer().map(str::to_owned)
        };
        let text = text.unwrap_or_default();
        self.ui.command_entry.set_text(&text);
        self.ui.command_entry.set_position(-1);
    }

    /// Tab (`forward`) or Shift+Tab: completes command names, folder paths
    /// and branch targets. Repeated presses cycle through the matches.
    pub(super) fn complete_command(&mut self, forward: bool, sender: &ComponentSender<Self>) {
        let text = self.ui.command_entry.text().to_string();
        if let Some(cycle) = &mut self.completion
            && cycle.is_current(&text)
        {
            let next = cycle.step(forward);
            let (candidates, index) = (cycle.candidates().to_vec(), cycle.index());
            self.set_command_text(&next);
            self.notice = Some(match_list(&candidates, index));
            return;
        }
        self.completion = None;
        let current = self.service.active_location().clone();
        match completion::plan(&text, &self.home, &current) {
            Plan::Nothing => {}
            Plan::Command { partial } => {
                let keep = text[..text.len() - partial.len()].to_owned();
                let names: Vec<String> = completion::command_candidates(&partial)
                    .into_iter()
                    .map(|n| format!("{n} "))
                    .collect();
                self.finish_completion(keep, &partial, names.clone(), names);
            }
            Plan::Branch { keep, partial } => {
                let ws = self.service.workspace();
                let names: Vec<String> = ws
                    .branches()
                    .iter()
                    .map(|b| {
                        ws.panel(b.tip())
                            .map(|p| folder_title(p.location()))
                            .unwrap_or_default()
                    })
                    .collect();
                let (targets, labels) = completion::branch_candidates(&partial, &names)
                    .into_iter()
                    .unzip();
                self.finish_completion(keep, &partial, targets, labels);
            }
            Plan::Path {
                keep,
                folder,
                partial,
            } => {
                // An open panel's listing is used as is; otherwise the
                // folder is read in the background (decision: no extra
                // reads for folders already shown).
                if let Some(listing) = self.service.listing_for(&folder) {
                    let names = completion::path_candidates(listing.entries(), &partial);
                    self.finish_completion(keep, &partial, names.clone(), names);
                    return;
                }
                self.completion_request += 1;
                let request = self.completion_request;
                let use_case = self.list_directory.clone();
                let sender = sender.clone();
                relm4::spawn_local(async move {
                    let result = use_case.execute(&folder).await;
                    sender.input_sender().emit(Msg::CompletionListed {
                        request,
                        text,
                        keep,
                        partial,
                        result: super::ListingResult(result),
                    });
                });
            }
        }
    }

    /// A background listing for completion arrived. It is used only if it
    /// is the latest request and the command line has not changed since.
    pub(super) fn completion_listed(
        &mut self,
        request: u64,
        text: &str,
        keep: String,
        partial: &str,
        result: Result<Vec<seshat_domain::entities::FileEntry>, ListingError>,
    ) {
        let visible = WidgetExt::is_visible(&self.ui.command_entry);
        if request != self.completion_request || !visible || self.ui.command_entry.text() != text {
            return;
        }
        match result {
            Ok(entries) => {
                let names = completion::path_candidates(&entries, partial);
                self.finish_completion(keep, partial, names.clone(), names);
            }
            Err(err) => self.notice = Some(crate::panel_view::error_message(&err, None)),
        }
    }

    /// Applies the matches: a single one is filled in; several fill in
    /// their common prefix, are listed, and start a Tab cycle.
    fn finish_completion(
        &mut self,
        keep: String,
        partial: &str,
        candidates: Vec<String>,
        labels: Vec<String>,
    ) {
        match candidates.as_slice() {
            [] => self.notice = Some("No matches".to_owned()),
            [single] => {
                self.set_command_text(&format!("{keep}{single}"));
                self.notice = None;
            }
            many => {
                let text = format!("{keep}{}", completion::common_prefix(many, partial));
                self.set_command_text(&text);
                self.notice = Some(match_list(&labels, None));
                self.completion = Some(CompletionCycle::new(keep, candidates, text));
            }
        }
    }

    fn set_command_text(&mut self, text: &str) {
        self.ui.command_entry.set_text(text);
        self.ui.command_entry.set_position(-1);
    }

    pub(super) fn command_submitted(&mut self, text: &str, sender: &ComponentSender<Self>) {
        self.command_history.push(text);
        let parsed = command::parse(text);
        self.close_command_line();
        match parsed {
            Ok(command) => {
                self.notice = None;
                self.run_command(command, sender);
            }
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
        }
    }

    fn run_command(&mut self, command: Command, sender: &ComponentSender<Self>) {
        // A command with a key-bindable equivalent goes through the same
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
            Command::Delete { permanently: false } => self.trash_selection(sender),
            Command::Delete { permanently: true } => self.delete_selection(sender),
            Command::Rename(name) => self.rename_selection(name, sender),
            Command::Mkdir(name) => self.new_item(true, Some(name), sender),
            Command::Touch(name) => self.new_item(false, Some(name), sender),
            Command::Search(query) => {
                self.service.set_query(active, &query);
                self.focused_panel = None;
            }
            Command::Sort { key, descending } => {
                self.service.set_sort_key(active, key);
                if let Some(descending) = descending
                    && self
                        .service
                        .state(active)
                        .is_some_and(|s| s.options.sort.descending != descending)
                {
                    self.service.toggle_sort_descending(active);
                }
                self.save_preferences();
            }
            Command::View(mode) => {
                self.service.set_view_mode(active, mode);
                self.save_preferences();
                self.focused_panel = None;
            }
            Command::Hidden => {
                self.service.toggle_hidden(active);
                self.save_preferences();
            }
            Command::Branch(BranchCommand::Overview) => self.show_branches(),
            Command::Branch(BranchCommand::Close) => self.close_branch(None),
            Command::Branch(BranchCommand::Switch(n)) => match self.branch_number(n) {
                Some(branch) => {
                    let _ = self.service.activate_branch(branch);
                }
                None => self.notice = Some(format!("There is no branch {n}.")),
            },
            Command::Open(text) => {
                let current = self.service.active_location().clone();
                match parse_address(&text, &self.home, Some(&current)) {
                    Ok(location) => self.service.open_location(location),
                    Err(err) => self.notice = Some(err.to_string()),
                }
            }
            Command::Help => self.show_help(),
            // Handled above by `perform` (see `Command::as_action`).
            Command::Branch(BranchCommand::Next | BranchCommand::Previous) | Command::Focus => {}
        }
    }

    fn branch_number(&self, n: usize) -> Option<BranchId> {
        self.service
            .workspace()
            .branches()
            .get(n.checked_sub(1)?)
            .map(|b| b.id())
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
            self.notice = Some("Select files first (v to select, V for a range).".to_owned());
            return;
        }
        let destination = match destination {
            Some(Destination::Branch(n)) => match self.branch_number(n) {
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
                    self.notice = Some(format!("There is no branch {n}."));
                    return;
                }
            },
            Some(Destination::Path(text)) => {
                let current = self.service.active_location().clone();
                match parse_address(&text, &self.home, Some(&current)) {
                    Ok(location) => Some(location),
                    Err(err) => {
                        self.notice = Some(err.to_string());
                        return;
                    }
                }
            }
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
                self.notice = Some("The items are already in that folder.".to_owned());
            }
            Ok(plan) => self.confirm_transfer(plan, sender),
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
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
        let title = format!("{}?", plan.summary());
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
            Err(err) => self.notice = Some(capitalise(&err.to_string())),
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

/// Completion matches shown in the status bar: at most this many.
const MAX_MATCHES_SHOWN: usize = 20;

/// The matches for the status bar, the selected one in brackets.
fn match_list(labels: &[String], current: Option<usize>) -> String {
    let mut shown: Vec<String> = labels
        .iter()
        .take(MAX_MATCHES_SHOWN)
        .enumerate()
        .map(|(i, label)| {
            if Some(i) == current {
                format!("[{label}]")
            } else {
                label.clone()
            }
        })
        .collect();
    if labels.len() > MAX_MATCHES_SHOWN {
        shown.push(format!("… {} more", labels.len() - MAX_MATCHES_SHOWN));
    }
    shown.join("  ")
}

#[cfg(test)]
mod tests {
    use super::match_list;

    #[test]
    fn match_lists_mark_the_current_one_and_are_capped() {
        let labels: Vec<String> = ["a/", "b/"].iter().map(|s| s.to_string()).collect();
        assert_eq!(match_list(&labels, None), "a/  b/");
        assert_eq!(match_list(&labels, Some(1)), "a/  [b/]");
        let many: Vec<String> = (0..25).map(|i| format!("d{i}/")).collect();
        let shown = match_list(&many, None);
        assert!(shown.ends_with("… 5 more"), "{shown}");
        assert!(!shown.contains("d20/"));
    }
}
