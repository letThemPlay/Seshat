//! The command line (`:`): its text field, history, and Tab completion of
//! command names, folder paths and branch targets (decisions P1–P4).
//! Running a command is the window's job (`App::run_command`), since
//! commands reach every part of it.

use std::path::PathBuf;

use gtk::prelude::*;
use seshat_application::interaction::command::{self, Command, CommandError, CommandHistory};
use seshat_application::interaction::completion::{self, CompletionCycle, Plan};
use seshat_application::ports::ListingError;
use seshat_application::use_cases::ListDirectory;
use seshat_domain::entities::FileEntry;

use super::{Cx, ListingResult, Msg};
use crate::notice::Notice;
use crate::workspace_view::folder_title;

/// Completion matches shown in the status bar: at most this many.
const MAX_MATCHES_SHOWN: usize = 20;

#[derive(Debug)]
pub enum CommandLineMsg {
    Submitted(String),
    Cancelled,
    /// Up (`older`) or Down: recall a previous command.
    History {
        older: bool,
    },
    /// Tab (`forward`) or Shift+Tab.
    Complete {
        forward: bool,
    },
    /// A folder listed for path completion.
    Listed {
        request: u64,
        text: String,
        keep: String,
        partial: String,
        result: ListingResult,
    },
}

/// What the window must do after a command-line message.
#[derive(Debug)]
pub(super) enum CommandLineOutcome {
    /// Nothing; the command line stays as it is.
    Stay,
    /// Close the command line (it was cancelled).
    Close,
    /// Close the command line and run what was typed.
    Run(Result<Command, CommandError>),
}

pub(super) struct CommandLineController {
    entry: gtk::Entry,
    history: CommandHistory,
    /// Tab cycling through completion matches (decision P1).
    completion: Option<CompletionCycle>,
    /// Numbers background listings for completion; late ones are dropped.
    request: u64,
    list_directory: ListDirectory,
    home: PathBuf,
}

impl CommandLineController {
    pub(super) fn new(entry: gtk::Entry, list_directory: ListDirectory, home: PathBuf) -> Self {
        Self {
            entry,
            history: CommandHistory::default(),
            completion: None,
            request: 0,
            list_directory,
            home,
        }
    }

    /// Opens the command line. From the keyboard, the key controller has
    /// already opened and focused it (so no typed key is lost); this only
    /// covers other ways in.
    pub(super) fn open(&self) {
        if WidgetExt::is_visible(&self.entry) {
            return;
        }
        self.entry.set_text("");
        self.entry.set_visible(true);
        self.entry.grab_focus();
    }

    pub(super) fn hide(&self) {
        self.entry.set_visible(false);
    }

    pub(super) fn update(&mut self, msg: CommandLineMsg, cx: &mut Cx<'_>) -> CommandLineOutcome {
        match msg {
            CommandLineMsg::Submitted(text) => {
                self.history.push(&text);
                return CommandLineOutcome::Run(command::parse(&text));
            }
            CommandLineMsg::Cancelled => return CommandLineOutcome::Close,
            CommandLineMsg::History { older } => self.history_step(older),
            CommandLineMsg::Complete { forward } => self.complete(forward, cx),
            CommandLineMsg::Listed {
                request,
                text,
                keep,
                partial,
                result,
            } => self.listed(request, &text, keep, &partial, result.0, cx.notice),
        }
        CommandLineOutcome::Stay
    }

    fn history_step(&mut self, older: bool) {
        let text = if older {
            self.history.older().map(str::to_owned)
        } else {
            self.history.newer().map(str::to_owned)
        };
        self.set_text(&text.unwrap_or_default());
    }

    /// Tab (`forward`) or Shift+Tab: completes command names, folder paths
    /// and branch targets. Repeated presses cycle through the matches.
    fn complete(&mut self, forward: bool, cx: &mut Cx<'_>) {
        let text = self.entry.text().to_string();
        if let Some(cycle) = &mut self.completion
            && cycle.is_current(&text)
        {
            let next = cycle.step(forward);
            let (candidates, index) = (cycle.candidates().to_vec(), cycle.index());
            self.set_text(&next);
            *cx.notice = Some(Notice::verbatim(match_list(&candidates, index)));
            return;
        }
        self.completion = None;
        let current = cx.service.active_location().clone();
        match completion::plan(&text, &self.home, &current) {
            Plan::Nothing => {}
            Plan::Command { partial } => {
                let keep = text[..text.len() - partial.len()].to_owned();
                let names: Vec<String> = completion::command_candidates(&partial)
                    .into_iter()
                    .map(|n| format!("{n} "))
                    .collect();
                self.finish(keep, &partial, names.clone(), names, cx.notice);
            }
            Plan::Branch { keep, partial } => {
                let ws = cx.service.workspace();
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
                self.finish(keep, &partial, targets, labels, cx.notice);
            }
            Plan::Path {
                keep,
                folder,
                partial,
            } => {
                // An open panel's listing is used as is; otherwise the
                // folder is read in the background (decision: no extra
                // reads for folders already shown).
                if let Some(listing) = cx.service.listing_for(&folder) {
                    let names = completion::path_candidates(listing.entries(), &partial);
                    self.finish(keep, &partial, names.clone(), names, cx.notice);
                    return;
                }
                self.request += 1;
                let request = self.request;
                let use_case = self.list_directory.clone();
                let sender = cx.sender.clone();
                relm4::spawn_local(async move {
                    let result = use_case.execute(&folder).await;
                    sender
                        .input_sender()
                        .emit(Msg::CommandLine(CommandLineMsg::Listed {
                            request,
                            text,
                            keep,
                            partial,
                            result: ListingResult(result),
                        }));
                });
            }
        }
    }

    /// A background listing for completion arrived. It is used only if it
    /// is the latest request and the command line has not changed since.
    fn listed(
        &mut self,
        request: u64,
        text: &str,
        keep: String,
        partial: &str,
        result: Result<Vec<FileEntry>, ListingError>,
        notice: &mut Option<Notice>,
    ) {
        if request != self.request
            || !WidgetExt::is_visible(&self.entry)
            || self.entry.text() != text
        {
            return;
        }
        match result {
            Ok(entries) => {
                let names = completion::path_candidates(&entries, partial);
                self.finish(keep, partial, names.clone(), names, notice);
            }
            Err(err) => *notice = Some(Notice::error(crate::panel_view::error_message(&err, None))),
        }
    }

    /// Applies the matches: a single one is filled in; several fill in
    /// their common prefix, are listed, and start a Tab cycle.
    fn finish(
        &mut self,
        keep: String,
        partial: &str,
        candidates: Vec<String>,
        labels: Vec<String>,
        notice: &mut Option<Notice>,
    ) {
        match candidates.as_slice() {
            [] => *notice = Some(Notice::info("No matches")),
            [single] => {
                self.set_text(&format!("{keep}{single}"));
                *notice = None;
            }
            many => {
                let text = format!("{keep}{}", completion::common_prefix(many, partial));
                self.set_text(&text);
                *notice = Some(Notice::verbatim(match_list(&labels, None)));
                self.completion = Some(CompletionCycle::new(keep, candidates, text));
            }
        }
    }

    fn set_text(&self, text: &str) {
        self.entry.set_text(text);
        self.entry.set_position(-1);
    }
}

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
