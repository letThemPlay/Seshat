//! Tab completion on the command line: command names, folder paths for
//! `:copy`, `:move` and `:open`, and branch targets (`b2`).
//!
//! Pure logic. The caller supplies the folder listing (from an open panel,
//! or read through the listing use case), so completing never touches the
//! filesystem from here. Paths are resolved with [`parse_address`], so `~`,
//! relative paths and `..` mean what they mean when the command runs.

use std::path::Path;

use seshat_domain::Direction;
use seshat_domain::entities::{FileEntry, Location};

use super::command::COMMANDS;
use crate::navigation::parse_address;
use crate::view::natural_cmp;

/// Commands whose argument is a folder (decision P2).
const PATH_COMMANDS: &[&str] = &["copy", "cp", "move", "mv", "open", "e", "edit"];
/// Commands whose argument may also be a branch target (decision P4).
const BRANCH_COMMANDS: &[&str] = &["copy", "cp", "move", "mv"];

/// What the text before the cursor asks to complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Nothing,
    /// The command name being typed.
    Command {
        partial: String,
    },
    /// A folder name: list `folder`, match `partial`, and put the result
    /// after `keep` (everything before the name being completed).
    Path {
        keep: String,
        folder: Location,
        partial: String,
    },
    /// A branch target such as `b2`.
    Branch {
        keep: String,
        partial: String,
    },
}

/// Decides what to complete in `input` (command-line text, with or without
/// the leading `:`). Relative paths are relative to `current`.
pub fn plan(input: &str, home: &Path, current: &Location) -> Plan {
    let body = input.trim_start().trim_start_matches(':');
    let lead = &input[..input.len() - body.len()];
    let Some(space) = body.find(char::is_whitespace) else {
        return Plan::Command {
            partial: body.to_owned(),
        };
    };
    let name = &body[..space];
    if !PATH_COMMANDS.contains(&name) {
        return Plan::Nothing;
    }
    let after = &body[space..];
    let argument = after.trim_start();
    let keep_command = format!("{lead}{name}{}", &after[..after.len() - argument.len()]);

    if BRANCH_COMMANDS.contains(&name)
        && let Some(digits) = argument.strip_prefix('b')
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        return Plan::Branch {
            keep: keep_command,
            partial: argument.to_owned(),
        };
    }
    if argument.starts_with("trash:") || argument.contains("://") {
        return Plan::Nothing; // the Trash is not a destination
    }
    // `~` alone completes to the home folder itself.
    let argument_for_split = if argument == "~" { "~/" } else { argument };
    let (folder_text, partial) = match argument_for_split.rfind('/') {
        Some(i) => argument_for_split.split_at(i + 1),
        None => ("", argument_for_split),
    };
    let folder = if folder_text.is_empty() {
        current.clone()
    } else {
        match parse_address(folder_text, home, Some(current)) {
            Ok(location) => location,
            Err(_) => return Plan::Nothing,
        }
    };
    if !matches!(folder, Location::Path(_)) {
        return Plan::Nothing;
    }
    Plan::Path {
        keep: format!("{keep_command}{folder_text}"),
        folder,
        partial: partial.to_owned(),
    }
}

/// Command names starting with `partial`.
pub fn command_candidates(partial: &str) -> Vec<String> {
    COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| name.starts_with(partial))
        .map(str::to_owned)
        .collect()
}

/// Folder names in `entries` matching `partial`, each with a trailing `/`.
///
/// - Only folders and links to folders (decision P2).
/// - Hidden folders only when `partial` starts with `.`.
/// - Smartcase (decision P3): an all-lowercase `partial` matches any case.
/// - Names that are not valid UTF-8 cannot be typed, so they are left out.
pub fn path_candidates(entries: &[FileEntry], partial: &str) -> Vec<String> {
    let insensitive = !partial.chars().any(char::is_uppercase);
    let wanted = if insensitive {
        partial.to_lowercase()
    } else {
        partial.to_owned()
    };
    let show_hidden = partial.starts_with('.');
    let mut names: Vec<String> = entries
        .iter()
        .filter(|e| e.is_browsable())
        .filter_map(|e| e.name.to_str())
        .filter(|name| show_hidden || !name.starts_with('.'))
        .filter(|name| {
            if insensitive {
                name.to_lowercase().starts_with(&wanted)
            } else {
                name.starts_with(&wanted)
            }
        })
        .map(|name| format!("{name}/"))
        .collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names
}

/// Branch targets matching `partial`: (`b2`, "b2 Downloads").
pub fn branch_candidates(partial: &str, branches: &[String]) -> Vec<(String, String)> {
    branches
        .iter()
        .enumerate()
        .map(|(i, name)| (format!("b{}", i + 1), name))
        .filter(|(target, _)| target.starts_with(partial))
        .map(|(target, name)| {
            let label = format!("{target} {name}");
            (target, label)
        })
        .collect()
}

/// The longest prefix all `candidates` share. A shorter result than
/// `partial` (only possible when matching ignored case) keeps `partial`.
pub fn common_prefix(candidates: &[String], partial: &str) -> String {
    let Some(first) = candidates.first() else {
        return partial.to_owned();
    };
    let mut end = first.len();
    for other in &candidates[1..] {
        end = first
            .char_indices()
            .zip(other.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8())
            .min(end);
    }
    let common = &first[..end];
    if common.chars().count() >= partial.chars().count() {
        common.to_owned()
    } else {
        partial.to_owned()
    }
}

/// Repeated Tab / Shift+Tab steps through the matches (decision P1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCycle {
    keep: String,
    candidates: Vec<String>,
    index: Option<usize>,
    /// The text last put in the command line, to notice other edits.
    shown: String,
}

impl CompletionCycle {
    /// Starts cycling after a Tab that left `shown` in the command line.
    pub fn new(keep: String, candidates: Vec<String>, shown: String) -> Self {
        debug_assert!(!candidates.is_empty(), "a cycle needs candidates");
        Self {
            keep,
            candidates,
            index: None,
            shown,
        }
    }

    /// Whether the command line still shows what this cycle put there.
    pub fn is_current(&self, text: &str) -> bool {
        self.shown == text
    }

    /// The next (or previous) match, as the full command-line text.
    pub fn step(&mut self, direction: Direction) -> String {
        let index = direction
            .wrapping_step(self.index, self.candidates.len())
            .expect("a cycle has candidates");
        self.index = Some(index);
        self.shown = format!("{}{}", self.keep, self.candidates[index]);
        self.shown.clone()
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    pub fn index(&self) -> Option<usize> {
        self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_domain::entities::EntryKind;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    fn home() -> &'static Path {
        Path::new("/h")
    }

    fn here() -> Location {
        Location::Path("/h/Documents".into())
    }

    fn entry(name: &str, kind: EntryKind) -> FileEntry {
        entry_os(name.into(), kind)
    }

    fn entry_os(name: OsString, kind: EntryKind) -> FileEntry {
        FileEntry {
            display_name: name.to_string_lossy().into_owned(),
            location: Location::Path(Path::new("/h").join(&name)),
            name,
            kind,
            size: None,
            modified: None,
            hidden: false,
            content_type: None,
            trash_info: None,
            identity: None,
        }
    }

    fn home_listing() -> Vec<FileEntry> {
        vec![
            entry("Desktop", EntryKind::Directory),
            entry("Documents", EntryKind::Directory),
            entry("Downloads", EntryKind::Directory),
            entry("docs.txt", EntryKind::File),
            entry(".config", EntryKind::Directory),
            entry("My Photos", EntryKind::Directory),
            entry("link", EntryKind::Symlink { to_directory: true }),
            entry(
                "broken",
                EntryKind::Symlink {
                    to_directory: false,
                },
            ),
            entry_os(
                OsString::from_vec(b"caf\xe9".to_vec()),
                EntryKind::Directory,
            ),
        ]
    }

    #[test]
    fn completion_command_lists_match_the_parser() {
        use crate::interaction::command::{Command, Destination, parse};
        for name in PATH_COMMANDS {
            let path = || Destination::Path("/x".into());
            assert!(
                matches!(
                    parse(&format!("{name} /x")),
                    Ok(Command::Open(p)) if p == "/x"
                ) || matches!(
                    parse(&format!("{name} /x")),
                    Ok(Command::Copy(Some(d)) | Command::Move(Some(d))) if d == path()
                ),
                "{name} does not take a folder"
            );
        }
        for name in BRANCH_COMMANDS {
            assert!(
                matches!(
                    parse(&format!("{name} b2")),
                    Ok(Command::Copy(Some(Destination::Branch(2)))
                        | Command::Move(Some(Destination::Branch(2))))
                ),
                "{name} does not take a branch"
            );
        }
    }

    #[test]
    fn plans_command_names_until_a_space() {
        assert_eq!(
            plan(":co", home(), &here()),
            Plan::Command {
                partial: "co".into()
            }
        );
        assert_eq!(command_candidates("co"), ["copy"]);
        assert_eq!(plan(":hidden x", home(), &here()), Plan::Nothing);
    }

    #[test]
    fn plans_paths_with_home_absolute_relative_and_parent() {
        let p = |input: &str| plan(input, home(), &here());
        assert_eq!(
            p(":open ~/Doc"),
            Plan::Path {
                keep: ":open ~/".into(),
                folder: Location::Path("/h".into()),
                partial: "Doc".into()
            }
        );
        assert_eq!(
            p("copy /mnt/us"),
            Plan::Path {
                keep: "copy /mnt/".into(),
                folder: Location::Path("/mnt".into()),
                partial: "us".into()
            }
        );
        assert_eq!(
            p(":move ../Dow"),
            Plan::Path {
                keep: ":move ../".into(),
                folder: Location::Path("/h".into()),
                partial: "Dow".into()
            }
        );
        assert_eq!(
            p(":open  Pro"),
            Plan::Path {
                keep: ":open  ".into(),
                folder: here(),
                partial: "Pro".into()
            },
            "relative to the active panel, spacing kept"
        );
        assert_eq!(
            p(":open ~"),
            Plan::Path {
                keep: ":open ~/".into(),
                folder: Location::Path("/h".into()),
                partial: String::new()
            }
        );
    }

    #[test]
    fn trash_and_other_schemes_are_not_completed() {
        assert_eq!(plan(":copy trash:", home(), &here()), Plan::Nothing);
        assert_eq!(plan(":open smb://x/", home(), &here()), Plan::Nothing);
        assert_eq!(
            plan(":open x", home(), &Location::trash_root()),
            Plan::Nothing
        );
    }

    #[test]
    fn plans_branch_targets_for_copy_and_move_only() {
        assert_eq!(
            plan(":copy b", home(), &here()),
            Plan::Branch {
                keep: ":copy ".into(),
                partial: "b".into()
            }
        );
        assert!(matches!(
            plan(":open b", home(), &here()),
            Plan::Path { .. }
        ));
        assert!(matches!(
            plan(":copy backup", home(), &here()),
            Plan::Path { .. }
        ));
        let names = ["Projects".to_owned(), "Downloads".to_owned()];
        assert_eq!(
            branch_candidates("b", &names),
            [
                ("b1".to_owned(), "b1 Projects".to_owned()),
                ("b2".to_owned(), "b2 Downloads".to_owned())
            ]
        );
        assert_eq!(branch_candidates("b2", &names).len(), 1);
    }

    #[test]
    fn candidates_are_folders_with_smartcase_and_hidden_rules() {
        let listing = home_listing();
        assert_eq!(
            path_candidates(&listing, "d"),
            ["Desktop/", "Documents/", "Downloads/"],
            "lowercase matches any case; files are left out"
        );
        assert!(path_candidates(&listing, "D").len() == 3);
        assert!(path_candidates(&listing, "Do").len() == 2);
        assert!(
            path_candidates(&listing, "dO").is_empty(),
            "a capital makes it exact"
        );
        assert_eq!(
            path_candidates(&listing, "l"),
            ["link/"],
            "links to folders count"
        );
        assert!(
            path_candidates(&listing, "b").is_empty(),
            "broken links do not"
        );
        assert!(
            path_candidates(&listing, "")
                .iter()
                .all(|c| !c.starts_with('.'))
        );
        assert_eq!(path_candidates(&listing, ".c"), [".config/"]);
        assert_eq!(
            path_candidates(&listing, "my"),
            ["My Photos/"],
            "spaces need no quoting"
        );
        assert!(
            path_candidates(&listing, "caf").is_empty(),
            "names that are not UTF-8 cannot be typed"
        );
    }

    #[test]
    fn common_prefix_extends_or_keeps_what_was_typed() {
        let c = |v: &[&str], p: &str| {
            common_prefix(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>(), p)
        };
        assert_eq!(c(&["Documents/"], "doc"), "Documents/");
        assert_eq!(c(&["Documents/", "Downloads/"], "d"), "Do");
        assert_eq!(
            c(&["Desktop/", "desk/"], "d"),
            "d",
            "case differs: keep what was typed"
        );
        assert_eq!(c(&[], "x"), "x");
        assert_eq!(c(&["Café/", "Cafés/"], "c"), "Café");
    }

    #[test]
    fn repeated_tab_cycles_both_ways_and_notices_edits() {
        let candidates = vec!["Desktop/".to_owned(), "Documents/".to_owned()];
        let mut cycle = CompletionCycle::new(":move ~/".into(), candidates, ":move ~/D".into());
        assert!(cycle.is_current(":move ~/D"));
        assert_eq!(cycle.step(Direction::Next), ":move ~/Desktop/");
        assert_eq!(cycle.step(Direction::Next), ":move ~/Documents/");
        assert_eq!(
            cycle.step(Direction::Next),
            ":move ~/Desktop/",
            "wraps around"
        );
        assert_eq!(cycle.step(Direction::Previous), ":move ~/Documents/");
        assert!(cycle.is_current(":move ~/Documents/"));
        assert!(
            !cycle.is_current(":move ~/Documents/x"),
            "typing ends the cycle"
        );

        let mut back = CompletionCycle::new(
            "k ".into(),
            vec!["a/".into(), "b/".into(), "c/".into()],
            "k ".into(),
        );
        assert_eq!(
            back.step(Direction::Previous),
            "k c/",
            "Shift+Tab starts from the last"
        );
    }
}
