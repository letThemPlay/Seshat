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
    file.set_len(256 * 1024 * 1024).unwrap(); // sparse; copied in 8 MiB chunks
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
