use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::Location;

/// What kind of filesystem object an entry is.
///
/// Symbolic links are reported as links and are never followed implicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Directory,
    /// A symbolic link. `to_directory` records whether its target is a
    /// directory (resolved once, so the link can be browsed into).
    Symlink {
        to_directory: bool,
    },
    Other,
}

impl EntryKind {
    /// Whether activating an entry of this kind opens it as a folder: a
    /// folder, or a link to one.
    pub fn is_browsable(self) -> bool {
        matches!(self, Self::Directory | Self::Symlink { to_directory: true })
    }
}

/// Where a trashed item came from (freedesktop.org Trash `.trashinfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashInfo {
    pub original_path: PathBuf,
    pub deleted_at: Option<SystemTime>,
}

/// A filesystem object's identity: device and inode. It tells "the same
/// folder, renamed" apart from "a different folder that now has this name".
/// Plain numbers, so the domain stays free of platform APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirectoryIdentity {
    pub device: u64,
    pub inode: u64,
}

/// A single entry of a directory listing.
///
/// Names are kept as OS strings: filenames on Linux are not guaranteed to be
/// valid UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: OsString,
    /// Name for display. Usually the lossy form of `name`; for Trash items it
    /// is the name GVfs presents, which can differ from the internal name.
    pub display_name: String,
    pub location: Location,
    pub kind: EntryKind,
    /// Size in bytes; `None` for directories, symlinks, or when unavailable.
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub hidden: bool,
    /// MIME type guessed from the name (never from contents), if known.
    pub content_type: Option<String>,
    /// Present only for top-level items in the Trash.
    pub trash_info: Option<TrashInfo>,
    /// Device and inode, when the filesystem reports them. Symbolic links
    /// are never followed, so a link has its own identity.
    pub identity: Option<DirectoryIdentity>,
}

impl FileEntry {
    pub fn is_directory(&self) -> bool {
        self.kind == EntryKind::Directory
    }

    /// Whether activating the entry should open it as a folder.
    pub fn is_browsable(&self) -> bool {
        self.kind.is_browsable()
    }

    /// The local path, if the entry lives on a regular filesystem.
    pub fn local_path(&self) -> Option<&Path> {
        match &self.location {
            Location::Path(path) => Some(path),
            Location::Trash(_) => None,
        }
    }

    /// Display-only name for a filename: invalid UTF-8 sequences replaced.
    /// Never use it to address the file.
    pub fn lossy_name(name: &std::ffi::OsStr) -> String {
        name.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn entry(name: OsString, location: Location) -> FileEntry {
        FileEntry {
            display_name: FileEntry::lossy_name(&name),
            name,
            location,
            kind: EntryKind::File,
            size: Some(1),
            modified: None,
            hidden: false,
            content_type: None,
            trash_info: None,
            identity: None,
        }
    }

    #[test]
    fn display_name_is_lossy_for_non_utf8_names() {
        let name = OsString::from_vec(b"report-\xff.txt".to_vec());
        let e = entry(
            name.clone(),
            Location::Path(PathBuf::from("/tmp").join(&name)),
        );
        assert_eq!(e.display_name, "report-\u{FFFD}.txt");
        assert!(!e.is_directory());
    }

    #[test]
    fn trash_entries_have_no_local_path() {
        let e = entry("a".into(), Location::Trash("a".into()));
        assert_eq!(e.local_path(), None);
        let e = entry("a".into(), Location::Path("/x/a".into()));
        assert_eq!(e.local_path(), Some(Path::new("/x/a")));
    }
}
