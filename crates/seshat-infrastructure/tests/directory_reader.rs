//! Integration tests for the GIO directory reader against a real filesystem.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use seshat_application::ports::{DirectoryReader, ListingError};
use seshat_domain::entities::{EntryKind, FileEntry, Location};
use seshat_infrastructure::filesystem::GioDirectoryReader;

fn read(location: &Location) -> Result<Vec<FileEntry>, ListingError> {
    let context = glib::MainContext::new();
    context.block_on(GioDirectoryReader::new().read_directory(location))
}

fn find<'a>(entries: &'a [FileEntry], name: &OsStr) -> &'a FileEntry {
    entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name:?} not listed"))
}

#[test]
fn lists_files_directories_symlinks_and_hidden_entries() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"hello").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join(".hidden"), b"").unwrap();
    symlink(dir.path().join("sub"), dir.path().join("link")).unwrap();

    let entries = read(&Location::Path(dir.path().to_path_buf())).unwrap();
    assert_eq!(entries.len(), 4);

    let file = find(&entries, OsStr::new("a.txt"));
    assert_eq!(file.kind, EntryKind::File);
    assert_eq!(file.size, Some(5));
    assert_eq!(file.local_path(), Some(dir.path().join("a.txt").as_path()));
    assert!(file.modified.is_some());

    let sub = find(&entries, OsStr::new("sub"));
    assert_eq!(sub.kind, EntryKind::Directory);
    assert_eq!(sub.size, None);

    // The link is reported as a link (not followed), but its target type is
    // known so it can be browsed.
    let link = find(&entries, OsStr::new("link"));
    assert_eq!(link.kind, EntryKind::Symlink { to_directory: true });
    assert!(link.is_browsable());
    assert_eq!(link.size, None);
    assert!(find(&entries, OsStr::new(".hidden")).hidden);
}

#[test]
fn preserves_non_utf8_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let name = OsStr::from_bytes(b"caf\xe9.txt");
    fs::write(dir.path().join(name), b"").unwrap();

    let entries = read(&Location::Path(dir.path().to_path_buf())).unwrap();
    let entry = find(&entries, name);
    assert_eq!(entry.local_path(), Some(dir.path().join(name).as_path()));
    assert_eq!(entry.trash_info, None);
}

#[test]
fn handles_large_directories() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..1_000 {
        fs::write(dir.path().join(format!("f{i}")), b"").unwrap();
    }
    let entries = read(&Location::Path(dir.path().to_path_buf())).unwrap();
    assert_eq!(entries.len(), 1_000);
}

#[test]
fn missing_directory_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let result = read(&Location::Path(dir.path().join("nope")));
    assert_eq!(result, Err(ListingError::NotFound));
}

#[test]
fn file_is_not_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("plain");
    fs::write(&file, b"").unwrap();
    assert_eq!(
        read(&Location::Path(file)),
        Err(ListingError::NotADirectory)
    );
}

#[test]
fn unreadable_directory_is_permission_denied() {
    let dir = tempfile::tempdir().unwrap();
    if fs::metadata(dir.path()).unwrap().uid() == 0 {
        return; // root bypasses permission checks
    }
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = read(&Location::Path(locked.clone()));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result, Err(ListingError::PermissionDenied));
}

/// Read-only: lists the session's real Trash through GVfs. Skips (passes)
/// when GVfs is not running, e.g. in CI containers.
#[test]
fn trash_root_lists_items_with_origin_or_reports_unavailable() {
    match read(&Location::trash_root()) {
        Ok(entries) => {
            for entry in &entries {
                assert!(entry.location.is_trash());
                assert!(entry.local_path().is_none());
                assert!(!entry.display_name.is_empty());
                let info = entry
                    .trash_info
                    .as_ref()
                    .expect("top-level trash items have origin");
                assert!(info.original_path.is_absolute());
            }
        }
        Err(ListingError::Unavailable(_) | ListingError::NotFound | ListingError::Other(_)) => {}
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn broken_and_file_symlinks_are_not_browsable() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file"), b"").unwrap();
    symlink(dir.path().join("file"), dir.path().join("to-file")).unwrap();
    symlink(dir.path().join("nowhere"), dir.path().join("broken")).unwrap();

    let entries = read(&Location::Path(dir.path().to_path_buf())).unwrap();
    for name in ["to-file", "broken"] {
        let entry = find(&entries, OsStr::new(name));
        assert_eq!(
            entry.kind,
            EntryKind::Symlink {
                to_directory: false
            },
            "{name}"
        );
        assert!(!entry.is_browsable());
    }
}
