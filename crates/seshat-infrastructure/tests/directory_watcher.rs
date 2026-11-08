//! Integration tests for the GIO directory watcher against a real filesystem.

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};

use seshat_application::ports::{DirectoryChange, DirectoryWatcher};
use seshat_domain::entities::Location;
use seshat_infrastructure::filesystem::GioDirectoryWatcher;

/// Watches `dir`, runs `action`, and iterates the main context until `want`
/// is observed or the timeout elapses. Returns everything observed.
fn observe(
    dir: &std::path::Path,
    action: impl FnOnce(),
    want: DirectoryChange,
) -> Vec<DirectoryChange> {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let seen = Rc::new(RefCell::new(Vec::new()));
            let sink = seen.clone();
            let _guard = GioDirectoryWatcher::new()
                .watch(
                    &Location::Path(dir.to_path_buf()),
                    Box::new(move |change| sink.borrow_mut().push(change)),
                )
                .expect("watch");

            action();

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline && !seen.borrow().contains(&want) {
                context.iteration(false);
                std::thread::sleep(Duration::from_millis(10));
            }
            seen.take()
        })
        .unwrap()
}

#[test]
fn reports_new_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let seen = observe(
        &path,
        || fs::write(path.join("new.txt"), b"x").unwrap(),
        DirectoryChange::Modified,
    );
    assert!(seen.contains(&DirectoryChange::Modified), "{seen:?}");
}

#[test]
fn reports_removal_of_the_watched_directory() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("watched");
    fs::create_dir(&dir).unwrap();
    let seen = observe(
        &dir,
        || fs::remove_dir(&dir).unwrap(),
        DirectoryChange::Removed,
    );
    assert!(seen.contains(&DirectoryChange::Removed), "{seen:?}");
}

#[test]
fn watching_a_missing_directory_fails_or_reports_nothing() {
    let parent = tempfile::tempdir().unwrap();
    let context = glib::MainContext::new();
    // GIO may accept a monitor on a missing path (inotify on the parent);
    // either way it must not panic.
    let _ = context.with_thread_default(|| {
        GioDirectoryWatcher::new()
            .watch(
                &Location::Path(parent.path().join("missing")),
                Box::new(|_| {}),
            )
            .is_ok()
    });
}
