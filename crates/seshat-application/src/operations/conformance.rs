//! Conformance checks for [`FileSystem`] implementations.
//!
//! Every file operation runs against the `FileSystem` port: the real
//! `LocalFileSystem` in the application, and `MemoryFs` in most tests.
//! Tests that pass against the test double only mean something if the
//! double behaves like the real thing. These checks state the contract
//! written on the trait as executable tests, and both implementations run
//! them (`run_all`), so a difference shows up as a failing test instead of
//! a bug that only happens on a real disk.
//!
//! Not covered: the Trash methods (a real implementation would touch the
//! session's own Trash) and `available_space` (its value depends on the
//! machine). Symbolic links point at absolute paths.
//!
//! Available with the `test-support` feature.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use seshat_domain::operation::EntryType;

use super::filesystem::{FileSystem, FsErrorKind};

/// A filesystem under test, with a way to set up and read back files the
/// [`FileSystem`] port itself cannot write.
pub trait Fixture {
    fn filesystem(&self) -> &dyn FileSystem;

    /// An existing, empty folder the checks work in. It must be a real path
    /// (no symbolic links in it), so canonical paths can be compared.
    fn root(&self) -> &Path;

    /// Creates a regular file with `content`.
    fn write_file(&self, path: &Path, content: &[u8]);

    /// Creates a symbolic link at `path` pointing at absolute `target`.
    fn symlink(&self, path: &Path, target: &Path);

    /// A regular file's content, or `None` if it isn't one.
    fn read_file(&self, path: &Path) -> Option<Vec<u8>>;
}

/// One contract check.
pub type Check = fn(&dyn Fixture);

/// Every check, with its name.
pub const CHECKS: &[(&str, Check)] = &[
    (
        "metadata reports type and size without following links",
        metadata_reports_type_and_size_without_following_links,
    ),
    ("read_dir lists entry names", read_dir_lists_entry_names),
    (
        "create_dir and create_file refuse existing paths",
        create_refuses_existing_paths,
    ),
    (
        "create needs an existing parent folder",
        create_needs_an_existing_parent,
    ),
    (
        "copy_file copies content and reports every byte",
        copy_file_copies_content_and_reports_progress,
    ),
    (
        "copy_file never replaces an existing target",
        copy_file_never_replaces_a_target,
    ),
    (
        "copy_file stops when cancelled",
        copy_file_stops_when_cancelled,
    ),
    (
        "copy_symlink copies the link, not its target",
        copy_symlink_copies_the_link,
    ),
    (
        "rename without replace keeps an existing target",
        rename_without_replace_keeps_the_target,
    ),
    (
        "rename with replace replaces the target",
        rename_with_replace_replaces_the_target,
    ),
    (
        "renaming a folder moves its contents",
        renaming_a_folder_moves_its_contents,
    ),
    (
        "remove_file removes a link, not its target",
        remove_file_removes_a_link_not_its_target,
    ),
    (
        "remove_empty_dir refuses a folder with contents",
        remove_empty_dir_refuses_contents,
    ),
    (
        "canonicalize resolves links, dot and dot-dot",
        canonicalize_resolves_links_and_dots,
    ),
];

/// Runs every check, each against a fresh fixture from `make`, and panics
/// listing all that failed.
pub fn run_all(make: &dyn Fn() -> Box<dyn Fixture>) {
    let mut failed = Vec::new();
    for (name, check) in CHECKS {
        let fixture = make();
        if catch_unwind(AssertUnwindSafe(|| check(fixture.as_ref()))).is_err() {
            failed.push(*name);
        }
    }
    assert!(
        failed.is_empty(),
        "FileSystem contract not met:\n  {}",
        failed.join("\n  ")
    );
}

fn at(fixture: &dyn Fixture, relative: &str) -> PathBuf {
    fixture.root().join(relative)
}

fn kind_of<T: std::fmt::Debug>(result: Result<T, super::FsError>) -> FsErrorKind {
    result.expect_err("should fail").kind
}

fn metadata_reports_type_and_size_without_following_links(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let file = at(fixture, "file");
    let folder = at(fixture, "folder");
    let link = at(fixture, "link");
    fixture.write_file(&file, b"hello");
    fs.create_dir(&folder).unwrap();
    fixture.symlink(&link, &file);

    let meta = fs.metadata(&file).unwrap();
    assert_eq!((meta.entry_type, meta.size), (EntryType::File, 5));
    assert_eq!(
        fs.metadata(&folder).unwrap().entry_type,
        EntryType::Directory
    );
    assert_eq!(fs.metadata(&link).unwrap().entry_type, EntryType::Symlink);
    assert_eq!(
        fs.metadata(&file).unwrap().device,
        fs.metadata(&folder).unwrap().device,
        "one filesystem, one device"
    );
    assert!(fs.exists(&file));
    assert!(!fs.exists(&at(fixture, "missing")));
    assert_eq!(
        kind_of(fs.metadata(&at(fixture, "missing"))),
        FsErrorKind::NotFound
    );
}

fn read_dir_lists_entry_names(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let folder = at(fixture, "folder");
    fs.create_dir(&folder).unwrap();
    fixture.write_file(&folder.join("a"), b"");
    fixture.write_file(&folder.join("b"), b"");
    fs.create_dir(&folder.join("sub")).unwrap();
    fixture.write_file(&folder.join("sub/deeper"), b"");

    let names: BTreeSet<OsString> = fs.read_dir(&folder).unwrap().into_iter().collect();
    let expected: BTreeSet<OsString> = ["a", "b", "sub"].into_iter().map(Into::into).collect();
    assert_eq!(names, expected, "direct entries only");
    assert!(fs.read_dir(&at(fixture, "missing")).is_err());
}

fn create_refuses_existing_paths(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let folder = at(fixture, "folder");
    let file = at(fixture, "file");
    fs.create_dir(&folder).unwrap();
    fs.create_file(&file).unwrap();
    assert_eq!(fs.metadata(&file).unwrap().size, 0, "created empty");

    assert_eq!(kind_of(fs.create_dir(&folder)), FsErrorKind::AlreadyExists);
    assert_eq!(kind_of(fs.create_dir(&file)), FsErrorKind::AlreadyExists);
    assert_eq!(kind_of(fs.create_file(&file)), FsErrorKind::AlreadyExists);
    assert_eq!(kind_of(fs.create_file(&folder)), FsErrorKind::AlreadyExists);
}

fn create_needs_an_existing_parent(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let orphan = at(fixture, "missing/child");
    assert_eq!(kind_of(fs.create_dir(&orphan)), FsErrorKind::NotFound);
    assert_eq!(kind_of(fs.create_file(&orphan)), FsErrorKind::NotFound);
}

fn copy_file_copies_content_and_reports_progress(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (from, to) = (at(fixture, "from"), at(fixture, "to"));
    fixture.write_file(&from, b"hello, world");
    let mut reported = 0;
    fs.copy_file(&from, &to, &mut |bytes| {
        reported += bytes;
        true
    })
    .unwrap();
    assert_eq!(
        fixture.read_file(&to).as_deref(),
        Some(&b"hello, world"[..])
    );
    assert_eq!(reported, 12, "every byte is reported");
    assert_eq!(
        fixture.read_file(&from).as_deref(),
        Some(&b"hello, world"[..]),
        "the source is untouched"
    );
}

fn copy_file_never_replaces_a_target(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (from, to) = (at(fixture, "from"), at(fixture, "to"));
    fixture.write_file(&from, b"new");
    fixture.write_file(&to, b"old");
    assert_eq!(
        kind_of(fs.copy_file(&from, &to, &mut |_| true)),
        FsErrorKind::AlreadyExists
    );
    assert_eq!(fixture.read_file(&to).as_deref(), Some(&b"old"[..]));
}

fn copy_file_stops_when_cancelled(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (from, to) = (at(fixture, "from"), at(fixture, "to"));
    fixture.write_file(&from, b"some content to copy");
    assert_eq!(
        kind_of(fs.copy_file(&from, &to, &mut |_| false)),
        FsErrorKind::Cancelled
    );
}

fn copy_symlink_copies_the_link(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let target = at(fixture, "target");
    let (link, copy) = (at(fixture, "link"), at(fixture, "copy"));
    fixture.write_file(&target, b"content");
    fixture.symlink(&link, &target);
    fs.copy_symlink(&link, &copy).unwrap();
    assert_eq!(fs.metadata(&copy).unwrap().entry_type, EntryType::Symlink);
    assert_eq!(fs.canonicalize(&copy).unwrap(), target);
    assert_eq!(
        kind_of(fs.copy_symlink(&link, &copy)),
        FsErrorKind::AlreadyExists
    );
}

fn rename_without_replace_keeps_the_target(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (from, to) = (at(fixture, "from"), at(fixture, "to"));
    fixture.write_file(&from, b"new");
    fixture.write_file(&to, b"old");
    assert_eq!(
        kind_of(fs.rename(&from, &to, false)),
        FsErrorKind::AlreadyExists
    );
    assert_eq!(fixture.read_file(&from).as_deref(), Some(&b"new"[..]));
    assert_eq!(fixture.read_file(&to).as_deref(), Some(&b"old"[..]));
}

fn rename_with_replace_replaces_the_target(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (from, to) = (at(fixture, "from"), at(fixture, "to"));
    fixture.write_file(&from, b"new");
    fixture.write_file(&to, b"old");
    fs.rename(&from, &to, true).unwrap();
    assert!(!fs.exists(&from));
    assert_eq!(fixture.read_file(&to).as_deref(), Some(&b"new"[..]));
}

fn renaming_a_folder_moves_its_contents(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (from, to) = (at(fixture, "from"), at(fixture, "to"));
    fs.create_dir(&from).unwrap();
    fixture.write_file(&from.join("inside"), b"x");
    fs.rename(&from, &to, false).unwrap();
    assert!(!fs.exists(&from));
    assert_eq!(
        fixture.read_file(&to.join("inside")).as_deref(),
        Some(&b"x"[..])
    );
}

fn remove_file_removes_a_link_not_its_target(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let (target, link) = (at(fixture, "target"), at(fixture, "link"));
    fixture.write_file(&target, b"keep me");
    fixture.symlink(&link, &target);
    fs.remove_file(&link).unwrap();
    assert!(!fs.exists(&link));
    assert_eq!(fixture.read_file(&target).as_deref(), Some(&b"keep me"[..]));
    assert_eq!(kind_of(fs.remove_file(&link)), FsErrorKind::NotFound);
}

fn remove_empty_dir_refuses_contents(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let folder = at(fixture, "folder");
    fs.create_dir(&folder).unwrap();
    fixture.write_file(&folder.join("inside"), b"");
    assert_eq!(kind_of(fs.remove_empty_dir(&folder)), FsErrorKind::NotEmpty);
    assert!(fs.exists(&folder.join("inside")), "nothing was removed");
    fs.remove_file(&folder.join("inside")).unwrap();
    fs.remove_empty_dir(&folder).unwrap();
    assert!(!fs.exists(&folder));
}

fn canonicalize_resolves_links_and_dots(fixture: &dyn Fixture) {
    let fs = fixture.filesystem();
    let real = at(fixture, "real");
    let other = at(fixture, "other");
    fs.create_dir(&real).unwrap();
    fs.create_dir(&other).unwrap();
    fixture.write_file(&real.join("file"), b"");
    fixture.symlink(&at(fixture, "alias"), &real);

    assert_eq!(fs.canonicalize(&real).unwrap(), real);
    assert_eq!(
        fs.canonicalize(&at(fixture, "alias/file")).unwrap(),
        real.join("file"),
        "a link in the middle of the path"
    );
    assert_eq!(
        fs.canonicalize(&at(fixture, "other/../real/./file"))
            .unwrap(),
        real.join("file"),
        "dot and dot-dot"
    );
    assert_eq!(
        kind_of(fs.canonicalize(&at(fixture, "missing"))),
        FsErrorKind::NotFound
    );
}
