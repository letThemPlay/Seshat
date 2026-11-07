//! Manager and algorithm tests against the in-memory filesystem.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use seshat_domain::entities::Location;
use seshat_domain::operation::{
    Conflict, ConflictDecision, ConflictResolution, Operation, OperationId, OperationStatus,
    Progress,
};

use super::memory_fs::{MemoryFs, Node};
use super::*;

const TIMEOUT: Duration = Duration::from_secs(5);

struct Harness {
    fs: Arc<MemoryFs>,
    manager: OperationManager,
    events: mpsc::Receiver<OperationEvent>,
}

fn harness(max_concurrent: usize) -> Harness {
    let fs = Arc::new(MemoryFs::new());
    let manager = OperationManager::new(fs.clone(), max_concurrent);
    let (tx, events) = mpsc::channel();
    let tx = Mutex::new(tx);
    manager.subscribe(Box::new(move |e| {
        let _ = tx.lock().unwrap().send(e.clone());
    }));
    Harness {
        fs,
        manager,
        events,
    }
}

impl Harness {
    fn submit(&self, request: OperationRequest) -> OperationId {
        self.manager.submit(request).expect("valid request")
    }

    /// Collects events until `id` reaches a terminal state.
    fn finish(&self, id: OperationId) -> (Operation, Vec<OperationEvent>) {
        let mut seen = Vec::new();
        loop {
            let event = self
                .events
                .recv_timeout(TIMEOUT)
                .expect("operation finished");
            let done = event.operation.id() == id && event.operation.is_finished();
            seen.push(event);
            if done {
                return (seen.last().unwrap().operation.clone(), seen);
            }
        }
    }

    fn run(&self, request: OperationRequest) -> Operation {
        let id = self.submit(request);
        self.finish(id).0
    }

    fn wait_for(&self, pred: impl Fn(&OperationEvent) -> bool) -> OperationEvent {
        loop {
            let event = self.events.recv_timeout(TIMEOUT).expect("event");
            if pred(&event) {
                return event;
            }
        }
    }
}

/// Answers conflicts from a script and records them.
struct Scripted {
    answers: Mutex<VecDeque<ConflictDecision>>,
    seen: Mutex<Vec<Conflict>>,
}

impl Scripted {
    fn new(answers: &[(ConflictResolution, bool)]) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(
                answers
                    .iter()
                    .map(|&(resolution, apply_to_all)| ConflictDecision {
                        resolution,
                        apply_to_all,
                    })
                    .collect(),
            ),
            seen: Mutex::new(Vec::new()),
        })
    }
}

impl ConflictResolver for Scripted {
    fn resolve(&self, op: &Operation, c: &Conflict, _: &CancelToken) -> ConflictDecision {
        assert!(op.awaiting_decision, "snapshot shows the wait");
        self.seen.lock().unwrap().push(c.clone());
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected conflict")
    }
}

fn copy(sources: &[&str], destination: &str) -> OperationRequest {
    OperationRequest::Copy {
        sources: sources.iter().map(PathBuf::from).collect(),
        destination: destination.into(),
    }
}

fn mv(sources: &[&str], destination: &str) -> OperationRequest {
    OperationRequest::Move {
        sources: sources.iter().map(PathBuf::from).collect(),
        destination: destination.into(),
    }
}

fn tree(fs: &MemoryFs) {
    fs.dir("/src");
    fs.dir("/src/docs");
    fs.file("/src/docs/a.txt", "hello world");
    fs.file("/src/docs/b.txt", "bee");
    fs.link("/src/docs/link", "/elsewhere");
    fs.dir("/dst");
}

// ── Lifecycle, events, snapshots ────────────────────────────────────────────

#[test]
fn copy_tree_reports_full_lifecycle_and_totals() {
    let h = harness(2);
    tree(&h.fs);
    let id = h.submit(copy(&["/src/docs"], "/dst"));
    let (op, events) = h.finish(id);

    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(
        h.fs.content("/dst/docs/a.txt").as_deref(),
        Some("hello world")
    );
    assert_eq!(
        h.fs.get("/dst/docs/link"),
        Some(Node::Link("/elsewhere".into()))
    );
    assert_eq!(
        op.files,
        Progress::Determinate {
            completed: 4,
            total: 4
        }
    );
    assert_eq!(
        op.bytes,
        Progress::Determinate {
            completed: 14,
            total: 14
        }
    );
    assert!(op.failures.is_empty());

    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds.first(), Some(&OperationEventKind::Queued));
    assert_eq!(kinds.get(1), Some(&OperationEventKind::Started));
    assert_eq!(kinds.last(), Some(&OperationEventKind::Completed));
    assert!(
        events
            .windows(2)
            .all(|w| w[1].sequence == w[0].sequence + 1),
        "no gaps"
    );
}

#[test]
fn snapshot_is_consistent_and_excludes_finished() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.set_gate(false);
    let id = h.submit(copy(&["/src/docs"], "/dst"));
    h.wait_for(|e| e.kind == OperationEventKind::Started);

    let snapshot = h.manager.snapshot();
    assert_eq!(snapshot.operations.len(), 1);
    assert_eq!(snapshot.operations[0].id(), id);
    // Every later event has a higher sequence than the snapshot.
    h.fs.set_gate(true);
    let (_, events) = h.finish(id);
    assert!(
        events
            .iter()
            .all(|e| e.sequence > snapshot.sequence || e.kind != OperationEventKind::Completed)
    );
    assert!(h.manager.snapshot().operations.is_empty());
    assert_eq!(
        h.manager.get(id).unwrap().status(),
        OperationStatus::Completed
    );
}

#[test]
fn invalid_requests_create_nothing() {
    let h = harness(1);
    assert!(h.manager.submit(copy(&["/a"], "/a/b")).is_err());
    assert!(h.events.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(h.manager.active_count(), 0);
}

#[test]
fn concurrency_is_bounded_and_queued_operations_can_be_cancelled() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.set_gate(false);
    let first = h.submit(copy(&["/src/docs/a.txt"], "/dst"));
    let second = h.submit(copy(&["/src/docs/b.txt"], "/dst"));
    h.wait_for(|e| e.operation.id() == first && e.kind == OperationEventKind::Started);
    assert_eq!(
        h.manager.get(second).unwrap().status(),
        OperationStatus::Queued
    );

    h.manager.cancel(second).unwrap();
    let cancelled = h.wait_for(|e| e.operation.id() == second && e.operation.is_finished());
    assert_eq!(cancelled.operation.status(), OperationStatus::Cancelled);
    assert_eq!(cancelled.operation.started_at, None, "never started");
    assert_eq!(h.manager.cancel(second), Err(CancelError::AlreadyFinished));

    h.fs.set_gate(true);
    assert_eq!(h.finish(first).0.status(), OperationStatus::Completed);
    assert!(!h.fs.paths().contains(&"/dst/b.txt".to_owned()));
}

#[test]
fn cancelling_a_running_copy_removes_the_partial_file() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.set_gate(false);
    let id = h.submit(copy(&["/src/docs/a.txt"], "/dst"));
    h.wait_for(|e| e.operation.current.is_some());
    h.manager.cancel(id).unwrap();
    let (op, events) = h.finish(id);
    assert_eq!(op.status(), OperationStatus::Cancelled);
    assert!(
        events
            .iter()
            .any(|e| e.kind == OperationEventKind::Cancelling)
    );
    assert!(!h.fs.paths().contains(&"/dst/a.txt".to_owned()));
}

#[test]
fn progress_events_are_throttled() {
    let h = harness(1);
    h.fs.dir("/many");
    for i in 0..500 {
        h.fs.file(&format!("/many/f{i}"), "x");
    }
    h.fs.dir("/dst");
    let id = h.submit(copy(&["/many"], "/dst"));
    let (op, events) = h.finish(id);
    assert_eq!(
        op.files,
        Progress::Determinate {
            completed: 501,
            total: 501
        }
    );
    let progress = events
        .iter()
        .filter(|e| e.kind == OperationEventKind::Progress)
        .count();
    assert!(progress < 100, "{progress} progress events for 501 items");
}

#[test]
fn insufficient_space_fails_before_writing() {
    let h = harness(1);
    tree(&h.fs);
    *h.fs.free_space.lock().unwrap() = Some(5);
    let op = h.run(copy(&["/src/docs"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Failed);
    assert!(op.error.unwrap().contains("Not enough free space"));
    assert!(!h.fs.paths().iter().any(|p| p.starts_with("/dst/")));
}

// ── Copy and conflicts ──────────────────────────────────────────────────────

#[test]
fn copying_into_the_same_folder_duplicates() {
    let h = harness(1);
    tree(&h.fs);
    let op = h.run(copy(&["/src/docs/a.txt"], "/src/docs"));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(
        h.fs.content("/src/docs/a (2).txt").as_deref(),
        Some("hello world")
    );
}

#[test]
fn conflict_skip_rename_and_overwrite() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.dir("/dst/docs");
    h.fs.file("/dst/docs/a.txt", "old a");
    h.fs.file("/dst/docs/b.txt", "old b");
    let resolver = Scripted::new(&[
        (ConflictResolution::Overwrite, false), // merge folder docs
        (ConflictResolution::Skip, false),      // a.txt
        (ConflictResolution::Rename, false),    // b.txt
    ]);
    h.manager.set_conflict_resolver(resolver.clone());
    let op = h.run(copy(&["/src/docs"], "/dst"));

    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(resolver.seen.lock().unwrap().len(), 3);
    assert_eq!(h.fs.content("/dst/docs/a.txt").as_deref(), Some("old a"));
    assert_eq!(h.fs.content("/dst/docs/b.txt").as_deref(), Some("old b"));
    assert_eq!(h.fs.content("/dst/docs/b (2).txt").as_deref(), Some("bee"));
    assert_eq!(op.files.completed(), 4, "skipped items count as processed");
}

#[test]
fn apply_to_all_answers_later_conflicts() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.file("/dst/a.txt", "old");
    h.fs.file("/dst/b.txt", "old");
    let resolver = Scripted::new(&[(ConflictResolution::Overwrite, true)]);
    h.manager.set_conflict_resolver(resolver.clone());
    let op = h.run(copy(&["/src/docs/a.txt", "/src/docs/b.txt"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(resolver.seen.lock().unwrap().len(), 1);
    assert_eq!(h.fs.content("/dst/a.txt").as_deref(), Some("hello world"));
    assert_eq!(h.fs.content("/dst/b.txt").as_deref(), Some("bee"));
    assert!(
        !h.fs.paths().iter().any(|p| p.contains("seshat-part")),
        "temp files removed"
    );
}

#[test]
fn cancel_from_conflict_dialog_cancels_the_operation() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.file("/dst/a.txt", "old");
    h.manager
        .set_conflict_resolver(Scripted::new(&[(ConflictResolution::Cancel, false)]));
    let id = h.submit(copy(&["/src/docs/a.txt", "/src/docs/b.txt"], "/dst"));
    let (op, events) = h.finish(id);
    assert_eq!(op.status(), OperationStatus::Cancelled);
    assert!(
        events
            .iter()
            .any(|e| e.kind == OperationEventKind::Cancelling)
    );
    assert_eq!(h.fs.content("/dst/a.txt").as_deref(), Some("old"));
    assert!(!h.fs.paths().contains(&"/dst/b.txt".to_owned()));
}

#[test]
fn failed_overwrite_keeps_the_original() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.file("/dst/a.txt", "precious");
    h.fs.fail_copies_of.lock().unwrap().insert("a.txt".into());
    h.manager
        .set_conflict_resolver(Scripted::new(&[(ConflictResolution::Overwrite, false)]));
    let op = h.run(copy(&["/src/docs/a.txt"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Failed, "the only item failed");
    assert_eq!(op.failures.len(), 1);
    assert_eq!(h.fs.content("/dst/a.txt").as_deref(), Some("precious"));
    assert!(!h.fs.paths().iter().any(|p| p.contains("seshat-part")));
}

#[test]
fn partial_failures_complete_with_a_list() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.fail_copies_of.lock().unwrap().insert("a.txt".into());
    let op = h.run(copy(&["/src/docs"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(op.failures.len(), 1);
    assert_eq!(op.failures[0].path, PathBuf::from("/src/docs/a.txt"));
    assert!(
        !h.fs.paths().contains(&"/dst/docs/a.txt".to_owned()),
        "partial removed"
    );
    assert_eq!(h.fs.content("/dst/docs/b.txt").as_deref(), Some("bee"));
}

#[test]
fn vanished_sources_are_reported_not_fatal() {
    let h = harness(1);
    tree(&h.fs);
    let op = h.run(copy(&["/src/docs/a.txt", "/src/gone.txt"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(op.failures.len(), 1);
}

// ── Move ────────────────────────────────────────────────────────────────────

#[test]
fn move_on_same_device_renames() {
    let h = harness(1);
    tree(&h.fs);
    let op = h.run(mv(&["/src/docs"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(
        h.fs.content("/dst/docs/a.txt").as_deref(),
        Some("hello world")
    );
    assert!(h.fs.get("/src/docs").is_none());
}

#[test]
fn move_across_devices_copies_then_removes() {
    let h = harness(1);
    tree(&h.fs);
    let op = h.run(mv(&["/src/docs"], "/other"));
    assert_eq!(op.status(), OperationStatus::Completed, "{:?}", op.failures);
    assert_eq!(h.fs.content("/other/docs/b.txt").as_deref(), Some("bee"));
    assert!(h.fs.get("/src/docs").is_none());
}

#[test]
fn failed_cross_device_move_keeps_the_source() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.fail_copies_of.lock().unwrap().insert("a.txt".into());
    let op = h.run(mv(&["/src/docs"], "/other"));
    assert_eq!(op.failures.len(), 1);
    assert_eq!(
        h.fs.content("/src/docs/a.txt").as_deref(),
        Some("hello world")
    );
    assert!(h.fs.get("/src/docs/b.txt").is_none(), "b was moved");
    assert!(
        h.fs.get("/src/docs").is_some(),
        "folder kept: not everything moved"
    );
}

#[test]
fn move_merge_into_existing_folder() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.dir("/dst/docs");
    h.fs.file("/dst/docs/keep.txt", "keep");
    h.manager
        .set_conflict_resolver(Scripted::new(&[(ConflictResolution::Overwrite, false)]));
    let op = h.run(mv(&["/src/docs"], "/dst"));
    assert_eq!(op.status(), OperationStatus::Completed, "{:?}", op.failures);
    assert_eq!(h.fs.content("/dst/docs/keep.txt").as_deref(), Some("keep"));
    assert_eq!(
        h.fs.content("/dst/docs/a.txt").as_deref(),
        Some("hello world")
    );
    assert!(h.fs.get("/src/docs").is_none());
}

// ── Delete, Trash, Restore ──────────────────────────────────────────────────

#[test]
fn delete_tree_without_following_links() {
    let h = harness(1);
    tree(&h.fs);
    h.fs.dir("/elsewhere");
    h.fs.file("/elsewhere/safe.txt", "safe");
    let op = h.run(OperationRequest::Delete {
        items: vec![Location::Path("/src/docs".into())],
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert!(h.fs.get("/src/docs").is_none());
    assert_eq!(h.fs.content("/elsewhere/safe.txt").as_deref(), Some("safe"));
}

#[test]
fn trash_failure_never_deletes() {
    let h = harness(1);
    tree(&h.fs);
    *h.fs.trash_unsupported.lock().unwrap() = true;
    let op = h.run(OperationRequest::Trash {
        paths: vec!["/src/docs/a.txt".into()],
    });
    assert_eq!(op.status(), OperationStatus::Failed);
    assert!(op.failures[0].message.contains("was not deleted"));
    assert_eq!(
        h.fs.content("/src/docs/a.txt").as_deref(),
        Some("hello world")
    );
}

#[test]
fn trash_then_restore_recreates_missing_parents() {
    let h = harness(1);
    tree(&h.fs);
    let op = h.run(OperationRequest::Trash {
        paths: vec!["/src/docs/a.txt".into()],
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(h.fs.trashed(), ["a.txt"]);

    let op = h.run(OperationRequest::Delete {
        items: vec![Location::Path("/src".into())],
    });
    assert_eq!(op.status(), OperationStatus::Completed);

    let op = h.run(OperationRequest::Restore {
        items: vec!["a.txt".into()],
    });
    assert_eq!(op.status(), OperationStatus::Completed, "{:?}", op.failures);
    assert_eq!(
        h.fs.content("/src/docs/a.txt").as_deref(),
        Some("hello world")
    );
    assert!(h.fs.trashed().is_empty());
}

#[test]
fn restore_conflict_can_keep_both() {
    let h = harness(1);
    tree(&h.fs);
    h.run(OperationRequest::Trash {
        paths: vec!["/src/docs/a.txt".into()],
    });
    h.fs.file("/src/docs/a.txt", "new");
    h.manager
        .set_conflict_resolver(Scripted::new(&[(ConflictResolution::Rename, false)]));
    let op = h.run(OperationRequest::Restore {
        items: vec!["a.txt".into()],
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(h.fs.content("/src/docs/a.txt").as_deref(), Some("new"));
    assert_eq!(
        h.fs.content("/src/docs/a (2).txt").as_deref(),
        Some("hello world")
    );
}

// ── Rename and create ───────────────────────────────────────────────────────

#[test]
fn rename_never_overwrites() {
    let h = harness(1);
    tree(&h.fs);
    let op = h.run(OperationRequest::Rename {
        path: "/src/docs/a.txt".into(),
        new_name: "b.txt".into(),
    });
    assert_eq!(op.status(), OperationStatus::Failed);
    assert_eq!(op.error.as_deref(), Some("“b.txt” already exists."));
    assert_eq!(h.fs.content("/src/docs/b.txt").as_deref(), Some("bee"));

    let op = h.run(OperationRequest::Rename {
        path: "/src/docs/a.txt".into(),
        new_name: "c.txt".into(),
    });
    assert_eq!(op.status(), OperationStatus::Completed);
    assert_eq!(
        h.fs.content("/src/docs/c.txt").as_deref(),
        Some("hello world")
    );
}

#[test]
fn create_folder_and_file() {
    let h = harness(1);
    h.fs.dir("/d");
    let dir = h.run(OperationRequest::CreateDirectory {
        parent: "/d".into(),
        name: "New Folder".into(),
    });
    assert_eq!(dir.status(), OperationStatus::Completed);
    assert_eq!(h.fs.get("/d/New Folder"), Some(Node::Dir));
    let again = h.run(OperationRequest::CreateDirectory {
        parent: "/d".into(),
        name: "New Folder".into(),
    });
    assert_eq!(again.status(), OperationStatus::Failed);
    let file = h.run(OperationRequest::CreateFile {
        parent: "/d".into(),
        name: "notes.txt".into(),
    });
    assert_eq!(file.status(), OperationStatus::Completed);
    assert_eq!(h.fs.content("/d/notes.txt").as_deref(), Some(""));
}
