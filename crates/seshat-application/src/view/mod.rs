//! How a listing is presented: filtering, search and sort order.
//! Pure functions over domain entries; no I/O.

mod natural;

pub use natural::{natural_cmp, natural_cmp_folded};
pub use seshat_domain::view::{SortKey, SortOrder, ViewMode, ViewOptions};

use std::cmp::Ordering;

use seshat_domain::entities::FileEntry;

/// Returns the entries to display, in display order.
///
/// Folders (and links to folders) always come first, regardless of the sort
/// direction. Equal keys fall back to the name, so the order is stable.
pub fn arrange(entries: &[FileEntry], options: &ViewOptions) -> Vec<FileEntry> {
    let query = SearchQuery::new(&options.query);
    // Case-fold each name once; search and every sort comparison reuse it.
    let mut visible: Vec<(&FileEntry, String)> = entries
        .iter()
        .filter(|e| options.show_hidden || !e.hidden)
        .map(|e| (e, e.display_name.to_lowercase()))
        .filter(|(_, folded)| query.matches(folded))
        .collect();
    visible.sort_by(|(a, fa), (b, fb)| compare(a, fa, b, fb, options.sort));
    visible.into_iter().map(|(e, _)| e.clone()).collect()
}

fn compare(a: &FileEntry, fa: &str, b: &FileEntry, fb: &str, sort: SortOrder) -> Ordering {
    let folders_first = b.is_browsable().cmp(&a.is_browsable());
    let by_name = || natural_cmp_folded(fa, fb).then_with(|| a.name.cmp(&b.name));
    let by_key = match sort.key {
        SortKey::Name => by_name(),
        SortKey::Size => a.size.cmp(&b.size).then_with(by_name),
        SortKey::Modified => a.modified.cmp(&b.modified).then_with(by_name),
        SortKey::Type => a.content_type.cmp(&b.content_type).then_with(by_name),
    };
    folders_first.then(if sort.descending {
        by_key.reverse()
    } else {
        by_key
    })
}

/// Case-insensitive search; every whitespace-separated term must occur.
struct SearchQuery {
    terms: Vec<String>,
}

impl SearchQuery {
    fn new(query: &str) -> Self {
        Self {
            terms: query.split_whitespace().map(str::to_lowercase).collect(),
        }
    }

    /// `folded` is the lowercased name.
    fn matches(&self, folded: &str) -> bool {
        self.terms.iter().all(|t| folded.contains(t.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::time::{Duration, SystemTime};

    use seshat_domain::entities::{EntryKind, Location};

    use super::*;

    fn entry(name: &str, kind: EntryKind) -> FileEntry {
        FileEntry {
            name: OsString::from(name),
            display_name: name.to_owned(),
            location: Location::Path(format!("/x/{name}").into()),
            kind,
            size: None,
            modified: None,
            hidden: name.starts_with('.'),
            content_type: None,
            trash_info: None,
            identity: None,
        }
    }

    fn file(name: &str, size: u64, age_secs: u64, mime: &str) -> FileEntry {
        FileEntry {
            size: Some(size),
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(age_secs)),
            content_type: Some(mime.to_owned()),
            ..entry(name, EntryKind::File)
        }
    }

    fn names(entries: &[FileEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.display_name.as_str()).collect()
    }

    fn sample() -> Vec<FileEntry> {
        vec![
            file("b.txt", 10, 3, "text/plain"),
            entry("Zeta", EntryKind::Directory),
            file("A.png", 300, 1, "image/png"),
            entry("alpha", EntryKind::Directory),
            entry("link", EntryKind::Symlink { to_directory: true }),
            file("c.md", 20, 2, "text/markdown"),
            entry(".config", EntryKind::Directory),
        ]
    }

    fn sorted(key: SortKey, descending: bool) -> Vec<FileEntry> {
        let options = ViewOptions {
            sort: SortOrder { key, descending },
            ..ViewOptions::default()
        };
        arrange(&sample(), &options)
    }

    #[test]
    fn default_is_folders_first_by_name_without_hidden() {
        let result = arrange(&sample(), &ViewOptions::default());
        assert_eq!(
            names(&result),
            ["alpha", "link", "Zeta", "A.png", "b.txt", "c.md"]
        );
    }

    #[test]
    fn hidden_entries_when_requested() {
        let options = ViewOptions {
            show_hidden: true,
            ..ViewOptions::default()
        };
        assert_eq!(names(&arrange(&sample(), &options))[0], ".config");
    }

    #[test]
    fn descending_keeps_folders_first() {
        assert_eq!(
            names(&sorted(SortKey::Name, true)),
            ["Zeta", "link", "alpha", "c.md", "b.txt", "A.png"]
        );
    }

    #[test]
    fn by_size_modified_and_type() {
        assert_eq!(
            &names(&sorted(SortKey::Size, false))[3..],
            ["b.txt", "c.md", "A.png"]
        );
        assert_eq!(
            &names(&sorted(SortKey::Size, true))[3..],
            ["A.png", "c.md", "b.txt"]
        );
        assert_eq!(
            &names(&sorted(SortKey::Modified, false))[3..],
            ["A.png", "c.md", "b.txt"]
        );
        assert_eq!(
            &names(&sorted(SortKey::Type, false))[3..],
            ["A.png", "c.md", "b.txt"]
        );
    }

    #[test]
    fn search_is_case_insensitive_and_requires_all_terms() {
        let search = |q: &str| {
            let options = ViewOptions {
                query: q.to_owned(),
                ..ViewOptions::default()
            };
            names(&arrange(&sample(), &options))
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(search("ALP"), ["alpha"]);
        assert_eq!(search("  .  t"), ["b.txt"]);
        assert_eq!(search("zzz"), Vec::<String>::new());
        assert_eq!(search("   ").len(), 6);
    }

    #[test]
    fn search_does_not_reveal_hidden_entries() {
        let options = ViewOptions {
            query: "config".to_owned(),
            ..ViewOptions::default()
        };
        assert!(arrange(&sample(), &options).is_empty());
    }
}
