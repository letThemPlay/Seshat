use std::ffi::OsString;
use std::path::PathBuf;
use std::time::SystemTime;

/// What kind of filesystem object an entry is.
///
/// Symbolic links are reported as links and are never followed implicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// A single entry of a directory listing.
///
/// Names and paths are kept as OS strings: filenames on Linux are not
/// guaranteed to be valid UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: OsString,
    pub path: PathBuf,
    pub kind: EntryKind,
    /// Size in bytes; `None` for directories or when unavailable.
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub hidden: bool,
}

impl FileEntry {
    pub fn is_directory(&self) -> bool {
        self.kind == EntryKind::Directory
    }

    /// Human readable name, with invalid UTF-8 sequences replaced.
    /// For display only — never use it to address the file.
    pub fn display_name(&self) -> String {
        self.name.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn display_name_is_lossy_for_non_utf8_names() {
        let name = OsString::from_vec(b"report-\xff.txt".to_vec());
        let entry = FileEntry {
            path: PathBuf::from("/tmp").join(&name),
            name,
            kind: EntryKind::File,
            size: Some(1),
            modified: None,
            hidden: false,
        };
        assert_eq!(entry.display_name(), "report-\u{FFFD}.txt");
        assert!(!entry.is_directory());
    }
}
