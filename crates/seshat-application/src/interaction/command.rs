use seshat_domain::view::{SortKey, ViewMode};
use thiserror::Error;

use super::{Action, AppAction};

/// Where `:copy` and `:move` put things (decision D10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// A typed folder path (`~/…` or absolute).
    Path(String),
    /// The active panel of branch number N (1-based, spatial order).
    Branch(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchCommand {
    /// Show the branch overview.
    Overview,
    Switch(usize),
    Next,
    Previous,
    Close,
}

/// A parsed command-line command. Each one runs through the same
/// application services as the equivalent key or menu action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `None` opens the destination picker.
    Copy(Option<Destination>),
    Move(Option<Destination>),
    /// `:delete` moves to the Trash; `:delete!` deletes permanently
    /// (always confirmed).
    Delete {
        permanently: bool,
    },
    Rename(Option<String>),
    Mkdir(String),
    Touch(String),
    /// An empty query clears the panel's search.
    Search(String),
    Sort {
        key: SortKey,
        descending: Option<bool>,
    },
    View(ViewMode),
    Hidden,
    Branch(BranchCommand),
    Focus,
    Open(String),
    Help,
}

impl Command {
    /// The window action this command is equivalent to, if any. Such
    /// commands are carried out by the same dispatcher as keys, menus and
    /// the toolbar, so they can never behave differently. Commands with an
    /// argument the window actions don't take (a path, a name, a query, a
    /// branch number, a sort direction) return `None`.
    pub fn as_action(&self) -> Option<AppAction> {
        Some(match self {
            Self::Branch(BranchCommand::Next) => Action::BranchNext.into(),
            Self::Branch(BranchCommand::Previous) => Action::BranchPrevious.into(),
            Self::Branch(BranchCommand::Overview) => AppAction::ShowBranches,
            Self::Branch(BranchCommand::Close) => AppAction::CloseBranch,
            Self::Focus => Action::FocusMode.into(),
            Self::Delete { permanently: false } => AppAction::Trash,
            Self::Delete { permanently: true } => AppAction::Delete,
            Self::Rename(None) => AppAction::Rename,
            Self::Sort {
                key,
                descending: None,
            } => AppAction::SetSortKey(*key),
            Self::View(mode) => AppAction::SetViewMode(*mode),
            Self::Hidden => AppAction::ToggleHidden,
            Self::Help => AppAction::Help,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    #[error("type a command, or press Esc")]
    Empty,
    #[error("unknown command “{0}” (try :help)")]
    Unknown(String),
    #[error("{0}")]
    Usage(&'static str),
}

/// Command names for completion and help, with a short description.
pub const COMMANDS: &[(&str, &str)] = &[
    ("copy", "Copy the selection: :copy [path | b<N>]"),
    ("move", "Move the selection: :move [path | b<N>]"),
    (
        "delete",
        "Move the selection to the Trash (:delete! deletes permanently)",
    ),
    ("rename", "Rename the item: :rename [new name]"),
    ("mkdir", "Create a folder: :mkdir <name>"),
    ("touch", "Create an empty file: :touch <name>"),
    ("search", "Search this panel: :search [text]"),
    ("sort", "Sort: :sort name|size|modified|type [asc|desc]"),
    ("view", "View: :view list|grid"),
    ("hidden", "Show or hide hidden files"),
    ("branch", "Branches: :branch [N | next | prev | close]"),
    ("focus", "Toggle Focus Mode"),
    ("open", "Open a location as a branch: :open <path>"),
    ("help", "Show commands and key bindings"),
];

/// Parses command-line input (without the leading `:`).
pub fn parse(input: &str) -> Result<Command, CommandError> {
    let input = input.trim().trim_start_matches(':').trim_start();
    let (name, rest) = match input.find(char::is_whitespace) {
        Some(i) => (&input[..i], input[i..].trim()),
        None => (input, ""),
    };
    let arg = (!rest.is_empty()).then(|| rest.to_owned());
    match name {
        "" => Err(CommandError::Empty),
        "copy" | "cp" => Ok(Command::Copy(arg.map(|a| destination(&a)))),
        "move" | "mv" => Ok(Command::Move(arg.map(|a| destination(&a)))),
        "delete" | "trash" => no_arg(rest, Command::Delete { permanently: false }),
        "delete!" => no_arg(rest, Command::Delete { permanently: true }),
        "rename" => Ok(Command::Rename(arg)),
        "mkdir" => arg
            .map(Command::Mkdir)
            .ok_or(CommandError::Usage("usage: :mkdir <name>")),
        "touch" => arg
            .map(Command::Touch)
            .ok_or(CommandError::Usage("usage: :touch <name>")),
        "search" => Ok(Command::Search(rest.to_owned())),
        "sort" => sort(rest),
        "view" => match rest {
            "list" => Ok(Command::View(ViewMode::List)),
            "grid" => Ok(Command::View(ViewMode::Grid)),
            _ => Err(CommandError::Usage("usage: :view list|grid")),
        },
        "hidden" => no_arg(rest, Command::Hidden),
        "branch" | "b" => branch(rest),
        "focus" => no_arg(rest, Command::Focus),
        "open" | "e" | "edit" => arg
            .map(Command::Open)
            .ok_or(CommandError::Usage("usage: :open <path>")),
        "help" | "h" => Ok(Command::Help),
        other => Err(CommandError::Unknown(other.to_owned())),
    }
}

fn no_arg(rest: &str, command: Command) -> Result<Command, CommandError> {
    if rest.is_empty() {
        Ok(command)
    } else {
        Err(CommandError::Usage("this command takes no arguments"))
    }
}

/// `b2` targets branch 2; anything else is a path.
fn destination(arg: &str) -> Destination {
    match arg
        .strip_prefix('b')
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|&n| n > 0)
    {
        Some(n) => Destination::Branch(n),
        None => Destination::Path(arg.to_owned()),
    }
}

fn sort(rest: &str) -> Result<Command, CommandError> {
    const USAGE: CommandError =
        CommandError::Usage("usage: :sort name|size|modified|type [asc|desc]");
    let mut words = rest.split_whitespace();
    let key = words.next().and_then(SortKey::from_id).ok_or(USAGE)?;
    let descending = match words.next() {
        None => None,
        Some("asc") => Some(false),
        Some("desc") => Some(true),
        Some(_) => return Err(USAGE),
    };
    if words.next().is_some() {
        return Err(USAGE);
    }
    Ok(Command::Sort { key, descending })
}

fn branch(rest: &str) -> Result<Command, CommandError> {
    let command = match rest {
        "" => BranchCommand::Overview,
        "next" => BranchCommand::Next,
        "prev" | "previous" => BranchCommand::Previous,
        "close" => BranchCommand::Close,
        n => match n.parse::<usize>() {
            Ok(n) if n > 0 => BranchCommand::Switch(n),
            _ => {
                return Err(CommandError::Usage(
                    "usage: :branch [N | next | prev | close]",
                ));
            }
        },
    };
    Ok(Command::Branch(command))
}

/// Command history with Up/Down recall, newest last.
#[derive(Debug, Default)]
pub struct CommandHistory {
    entries: Vec<String>,
    /// Position while browsing with Up/Down; `None` when not browsing.
    position: Option<usize>,
}

const HISTORY_LIMIT: usize = 100;

impl CommandHistory {
    pub fn push(&mut self, command: &str) {
        let command = command.trim();
        self.position = None;
        if command.is_empty() || self.entries.last().is_some_and(|l| l == command) {
            return;
        }
        self.entries.push(command.to_owned());
        if self.entries.len() > HISTORY_LIMIT {
            self.entries.remove(0);
        }
    }

    pub fn older(&mut self) -> Option<&str> {
        let next = match self.position {
            None => self.entries.len().checked_sub(1)?,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.position = Some(next);
        self.entries.get(next).map(String::as_str)
    }

    /// The newer entry, or `None` (an empty line) past the newest.
    pub fn newer(&mut self) -> Option<&str> {
        let i = self.position? + 1;
        if i >= self.entries.len() {
            self.position = None;
            return None;
        }
        self.position = Some(i);
        self.entries.get(i).map(String::as_str)
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_with_a_window_action_map_to_that_action() {
        let action = |input: &str| parse(input).expect("parses").as_action();
        assert_eq!(action("branch next"), Some(Action::BranchNext.into()));
        assert_eq!(action("branch prev"), Some(Action::BranchPrevious.into()));
        assert_eq!(action("branch"), Some(AppAction::ShowBranches));
        assert_eq!(action("branch close"), Some(AppAction::CloseBranch));
        assert_eq!(action("focus"), Some(Action::FocusMode.into()));
        assert_eq!(action("delete"), Some(AppAction::Trash));
        assert_eq!(action("delete!"), Some(AppAction::Delete));
        assert_eq!(action("rename"), Some(AppAction::Rename));
        assert_eq!(
            action("sort size"),
            Some(AppAction::SetSortKey(SortKey::Size))
        );
        assert_eq!(
            action("view grid"),
            Some(AppAction::SetViewMode(ViewMode::Grid))
        );
        assert_eq!(action("hidden"), Some(AppAction::ToggleHidden));
        assert_eq!(action("help"), Some(AppAction::Help));
        // Arguments the window actions don't take.
        for input in [
            "branch 2",
            "copy",
            "move ~",
            "rename x",
            "mkdir x",
            "touch x",
            "search x",
            "sort size desc",
            "open ~",
        ] {
            assert_eq!(action(input), None, "{input}");
        }
    }

    #[test]
    fn parses_the_spec_commands() {
        assert_eq!(parse("copy"), Ok(Command::Copy(None)));
        assert_eq!(
            parse(":copy ~/Backup"),
            Ok(Command::Copy(Some(Destination::Path("~/Backup".into()))))
        );
        assert_eq!(
            parse("move b2"),
            Ok(Command::Move(Some(Destination::Branch(2))))
        );
        assert_eq!(parse("delete"), Ok(Command::Delete { permanently: false }));
        assert_eq!(parse("delete!"), Ok(Command::Delete { permanently: true }));
        assert_eq!(parse("rename"), Ok(Command::Rename(None)));
        assert_eq!(
            parse("rename new name.txt"),
            Ok(Command::Rename(Some("new name.txt".into())))
        );
        assert_eq!(parse("mkdir Photos"), Ok(Command::Mkdir("Photos".into())));
        assert_eq!(
            parse("search  foo bar "),
            Ok(Command::Search("foo bar".into()))
        );
        assert_eq!(parse("search"), Ok(Command::Search(String::new())));
        assert_eq!(
            parse("sort size desc"),
            Ok(Command::Sort {
                key: SortKey::Size,
                descending: Some(true)
            })
        );
        assert_eq!(parse("view grid"), Ok(Command::View(ViewMode::Grid)));
        assert_eq!(parse("hidden"), Ok(Command::Hidden));
        assert_eq!(
            parse("branch"),
            Ok(Command::Branch(BranchCommand::Overview))
        );
        assert_eq!(
            parse("branch close"),
            Ok(Command::Branch(BranchCommand::Close))
        );
        assert_eq!(
            parse("branch 3"),
            Ok(Command::Branch(BranchCommand::Switch(3)))
        );
        assert_eq!(parse("focus"), Ok(Command::Focus));
        assert_eq!(parse("help"), Ok(Command::Help));
    }

    #[test]
    fn reports_clear_errors() {
        assert_eq!(parse("  "), Err(CommandError::Empty));
        assert_eq!(
            parse("frobnicate"),
            Err(CommandError::Unknown("frobnicate".into()))
        );
        assert!(matches!(parse("mkdir"), Err(CommandError::Usage(_))));
        assert!(matches!(parse("sort colour"), Err(CommandError::Usage(_))));
        assert!(matches!(parse("view tiles"), Err(CommandError::Usage(_))));
        assert!(matches!(parse("branch 0"), Err(CommandError::Usage(_))));
        assert!(matches!(parse("focus now"), Err(CommandError::Usage(_))));
    }

    #[test]
    fn a_folder_named_like_b_is_a_path() {
        assert_eq!(
            parse("copy backup"),
            Ok(Command::Copy(Some(Destination::Path("backup".into()))))
        );
    }

    #[test]
    fn history_recalls_with_up_and_down() {
        let mut h = CommandHistory::default();
        h.push("mkdir a");
        h.push("mkdir a"); // repeated: kept once
        h.push("focus");
        assert_eq!(h.older(), Some("focus"));
        assert_eq!(h.older(), Some("mkdir a"));
        assert_eq!(h.older(), Some("mkdir a"), "stays at the oldest");
        assert_eq!(h.newer(), Some("focus"));
        assert_eq!(h.newer(), None);
        assert_eq!(h.entries().len(), 2);
    }
}
