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
    assert_eq!(file.path, dir.path().join("a.txt"));
    assert!(file.modified.is_some());

    let sub = find(&entries, OsStr::new("sub"));
    assert_eq!(sub.kind, EntryKind::Directory);
    assert_eq!(sub.size, None);

    // The link points at a directory but must not be followed.
    assert_eq!(find(&entries, OsStr::new("link")).kind, EntryKind::Symlink);
    assert!(find(&entries, OsStr::new(".hidden")).hidden);
}

#[test]
fn preserves_non_utf8_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let name = OsStr::from_bytes(b"caf\xe9.txt");
    fs::write(dir.path().join(name), b"").unwrap();

    let entries = read(&Location::Path(dir.path().to_path_buf())).unwrap();
    let entry = find(&entries, name);
    assert_eq!(entry.path, dir.path().join(name));
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
