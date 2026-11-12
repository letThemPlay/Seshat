//! Searching inside a typed folder (Spec 4 §6.4, decision F10).
//!
//! A query starting with `/`, `~/` or exactly `~` names a folder: the text
//! up to its last `/` is the folder, searched as the
//! [`SearchScope::AbsolutePath`](seshat_domain::search::SearchScope)
//! scope, and the rest is the fuzzy pattern. The folder is read with the
//! address bar's own rules ([`parse_address`]), so `~`, `.` and `..` mean
//! the same in both places. Whether the folder exists is reported by the
//! walk, like any other scope.

use std::path::{Path, PathBuf};

use seshat_domain::entities::Location;

use crate::navigation::parse_address;

/// A query that names a folder to search in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathQuery {
    /// The folder, absolute and with `.` and `..` resolved.
    pub folder: PathBuf,
    /// What to look for inside it; may be empty.
    pub pattern: String,
}

impl PathQuery {
    /// `text` read as a folder and a pattern, or `None` for an ordinary
    /// query. `~` followed by anything but `/` (`~user`) is not a path:
    /// only the user's own Home is understood, as in the address bar.
    pub fn parse(text: &str, home: &Path) -> Option<Self> {
        let text = text.trim_start();
        if !(text.starts_with('/') || text == "~" || text.starts_with("~/")) {
            return None;
        }
        let (folder, pattern) = match text.rfind('/') {
            Some(slash) => (&text[..=slash], &text[slash + 1..]),
            None => (text, ""), // just "~"
        };
        match parse_address(folder, home, None) {
            Ok(Location::Path(folder)) => Some(Self {
                folder,
                pattern: pattern.to_owned(),
            }),
            // Absolute paths and `~` always parse to a path.
            Ok(Location::Trash(_)) | Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<(String, String)> {
        PathQuery::parse(text, Path::new("/home/u"))
            .map(|q| (q.folder.display().to_string(), q.pattern))
    }

    fn some(folder: &str, pattern: &str) -> Option<(String, String)> {
        Some((folder.to_owned(), pattern.to_owned()))
    }

    #[test]
    fn the_text_up_to_the_last_slash_is_the_folder() {
        assert_eq!(parse("/usr/share/ico"), some("/usr/share", "ico"));
        assert_eq!(parse("/usr/share/"), some("/usr/share", ""));
        assert_eq!(parse("/"), some("/", ""));
        assert_eq!(parse("/etc"), some("/", "etc"));
        assert_eq!(
            parse("/srv/my files/two words"),
            some("/srv/my files", "two words")
        );
    }

    #[test]
    fn home_is_understood_as_in_the_address_bar() {
        assert_eq!(parse("~"), some("/home/u", ""));
        assert_eq!(parse("~/"), some("/home/u", ""));
        assert_eq!(parse("~/Doc"), some("/home/u", "Doc"));
        assert_eq!(parse("~/Documents/../Down"), some("/home/u", "Down"));
        assert_eq!(parse("/a/./b/../c/x"), some("/a/c", "x"));
    }

    #[test]
    fn other_queries_are_ordinary_patterns() {
        assert_eq!(parse("notes"), None);
        assert_eq!(parse("src/main"), None, "a relative path is a pattern");
        assert_eq!(parse("~user/x"), None, "other users' Homes aren't paths");
        assert_eq!(parse(""), None);
        assert_eq!(parse("  /tmp/x"), some("/tmp", "x"), "leading spaces");
    }
}
