//! The operation manager running against the real filesystem.
//!
//! Trash operations are not exercised here: they would touch the session's
//! real Trash. They are covered by the in-memory tests in the application
//! crate and by the manual checklist.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use seshat_application::operations::{
    FileSystem, OperationEvent, OperationEventKind, OperationManager, OperationRequest,
};
use seshat_domain::entities::Location;
use seshat_domain::operation::{Operation, OperationStatus};
use seshat_infrastructure::filesystem::LocalFileSystem;

fn manager() -> (OperationManager, mpsc::Receiver<OperationEvent>) {
    let manager = OperationManager::new(Arc::new(LocalFileSystem::new()), 2);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    manager.subscribe(Box::new(move |e| {
        let _ = tx.lock().unwrap().send(e.clone());
    }));
    (manager, rx)
}

fn wait(
    rx: &mpsc::Receiver<OperationEvent>,
    id: seshat_domain::operation::OperationId,
) -> Operation {
    loop {
        let event = rx.recv_timeout(Duration::from_secs(30)).expect("finished");
        if event.operation.id() == id && event.operation.is_finished() {
            return event.operation;
        }
    }
}

fn run(request: OperationRequest) -> Operation {
    let (manager, rx) = manager();
    let id = manager.submit(request).unwrap();
    wait(&rx, id)
}

fn copy(sources: &[&Path], destination: &Path) -> OperationRequest {
    OperationRequest::Copy {
        sources: sources.iter().map(|p| p.to_path_buf()).collect(),
        destination: destination.to_path_buf(),
    }
}

#[test]
fn copy_preserves_content_permissions_mtime_and_links() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    let file = src.join("script.sh");
    fs::write(&file, b"#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o750)).unwrap();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(old)
        .unwrap();
    symlink("script.sh", src.join("link")).unwrap();
    let dst = dir.path().join("dst");
    fs::create_dir(&dst).unwrap();

    let op = run(copy(&[&src], &dst));
    assert_eq!(op.status(), OperationStatus::Completed, "{:?}", op.failures);

    let copied = dst.join("src/script.sh");
    assert_eq!(fs::read(&copied).unwrap(), b"#!/bin/sh\necho hi\n");
    let meta = fs::metadata(&copied).unwrap();
    assert_eq!(meta.mode() & 0o777, 0o750);
    assert_eq!(meta.modified().unwrap(), old);
    let link = dst.join("src/link");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("script.sh"));
}

#[test]
fn rename_does_not_replace_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a"), b"a").unwrap();
    fs::write(dir.path().join("b"), b"b").unwrap();
    let op = run(OperationRequest::Rename {
        path: dir.path().join("a"),
        new_name: "b".into(),
    });
    assert_eq!(op.status(), OperationStatus::Failed);
    assert_eq!(fs::read(dir.path().join("b")).unwrap(), b"b");
    assert_eq!(fs::read(dir.path().join("a")).unwrap(), b"a");
}

#[test]
fn unreadable_files_are_partial_failures() {
    let dir = tempfile::tempdir().unwrap();
    if fs::metadata(dir.path()).unwrap().uid() == 0 {
        return; // root ignores permissions
    }
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("ok.txt"), b"ok").unwrap();
    fs::write(src.join("secret.txt"), b"no").unwrap();
    fs::set_permissions(src.join("secret.txt"), fs::Permissions::from_mode(0o000)).unwrap();
    let dst = dir.path().join("dst");
    fs::create_dir(&dst).unwrap();

    let op = run(copy(&[&src], &dst));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(op.failures.len(), 1);
    assert!(op.failures[0].path.ends_with("secret.txt"));
    assert_eq!(fs::read(dst.join("src/ok.txt")).unwrap(), b"ok");
    assert!(!dst.join("src/secret.txt").exists(), "no partial file left");
}

#[test]
fn move_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("folder");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("nested/f"), b"f").unwrap();
    let dst = dir.path().join("dst");
    fs::create_dir(&dst).unwrap();

    let op = run(OperationRequest::Move {
        sources: vec![src.clone()],
        destination: dst.clone(),
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert!(!src.exists());
    assert_eq!(fs::read(dst.join("folder/nested/f")).unwrap(), b"f");

    let op = run(OperationRequest::Delete {
        items: vec![Location::Path(dst.join("folder"))],
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert!(!dst.join("folder").exists());
}

#[test]
fn delete_does_not_follow_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), b"keep").unwrap();
    let doomed = dir.path().join("doomed");
    fs::create_dir(&doomed).unwrap();
    symlink(&outside, doomed.join("link-to-outside")).unwrap();

    let op = run(OperationRequest::Delete {
        items: vec![Location::Path(doomed.clone())],
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert!(!doomed.exists());
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");
}

#[test]
fn cancelling_a_large_copy_leaves_no_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big.bin");
    let file = fs::File::create(&big).unwrap();
    file.set_len(256 * 1024 * 1024).unwrap(); // sparse; large enough to cancel mid-copy
    let dst = dir.path().join("dst");
    fs::create_dir(&dst).unwrap();

    let (manager, rx) = manager();
    let id = manager.submit(copy(&[&big], &dst)).unwrap();
    loop {
        let event = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if event.kind == OperationEventKind::Started {
            break;
        }
    }
    manager.cancel(id).unwrap();
    let op = wait(&rx, id);
    // A fast machine may finish first; either way nothing partial remains.
    match op.status() {
        OperationStatus::Cancelled => assert!(!dst.join("big.bin").exists()),
        OperationStatus::Completed => {
            assert_eq!(
                fs::metadata(dst.join("big.bin")).unwrap().len(),
                256 * 1024 * 1024
            )
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn available_space_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    assert!(LocalFileSystem::new().available_space(dir.path()).is_some());
}

#[test]
fn copies_drop_setuid_and_setgid() {
    let dir = tempfile::tempdir().unwrap();
    let tool = dir.path().join("tool");
    fs::write(&tool, b"#!/bin/sh\n").unwrap();
    // Some environments cannot create setuid files at all: build sandboxes
    // (seccomp rejects the chmod) or nosuid filesystems. Nothing to test then.
    if fs::set_permissions(&tool, fs::Permissions::from_mode(0o6755)).is_err()
        || fs::metadata(&tool).unwrap().mode() & 0o6000 == 0
    {
        eprintln!("cannot create setuid files here; skipping");
        return;
    }
    let dst = dir.path().join("dst");
    fs::create_dir(&dst).unwrap();
    let op = run(copy(&[&tool], &dst));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(
        fs::metadata(dst.join("tool")).unwrap().mode() & 0o7777,
        0o755
    );
}

#[test]
fn copied_folders_keep_the_sticky_bit() {
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    fs::create_dir(&shared).unwrap();
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777)).unwrap();
    let dst = dir.path().join("dst");
    fs::create_dir(&dst).unwrap();
    let op = run(copy(&[&shared], &dst));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(
        fs::metadata(dst.join("shared")).unwrap().mode() & 0o7777,
        0o1777
    );
}

#[test]
fn copy_into_symlinked_folder_and_alias_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    symlink(&real, dir.path().join("shortcut")).unwrap();
    fs::write(dir.path().join("a.txt"), b"a").unwrap();
    let op = run(copy(
        &[&dir.path().join("a.txt")],
        &dir.path().join("shortcut"),
    ));
    assert_eq!(op.status(), OperationStatus::Completed, "{:?}", op.error);
    assert_eq!(fs::read(real.join("a.txt")).unwrap(), b"a");

    // /alias -> real/inner: copying `real` into it would recurse forever.
    fs::create_dir(real.join("inner")).unwrap();
    symlink(real.join("inner"), dir.path().join("alias")).unwrap();
    let op = run(copy(&[&real], &dir.path().join("alias")));
    assert_eq!(op.status(), OperationStatus::Failed);
    assert!(!real.join("inner/real").exists());
}

#[test]
fn listing_refuses_to_follow_a_symlinked_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target/precious"), b"x").unwrap();
    symlink(dir.path().join("target"), dir.path().join("link")).unwrap();
    let fs_ops = LocalFileSystem::new();
    assert!(fs_ops.read_dir(&dir.path().join("link")).is_err());
    assert_eq!(
        fs_ops.read_dir(&dir.path().join("target")).unwrap().len(),
        1
    );
}
