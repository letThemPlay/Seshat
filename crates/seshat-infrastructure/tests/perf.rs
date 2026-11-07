//! Performance measurements (not assertions of exact numbers). Run with
//! `cargo test --release -p seshat-infrastructure --test perf -- --ignored --nocapture`.

use std::fs;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use seshat_application::operations::{OperationEventKind, OperationManager, OperationRequest};
use seshat_application::ports::DirectoryReader;
use seshat_application::view::{SortKey, SortOrder, ViewOptions, arrange};
use seshat_domain::entities::Location;
use seshat_infrastructure::filesystem::{GioDirectoryReader, LocalFileSystem};

const ENTRIES: usize = 100_000;

#[test]
#[ignore = "benchmark"]
fn large_directory_listing_and_arranging() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..ENTRIES {
        fs::File::create(dir.path().join(format!("file-{i}.txt"))).unwrap();
    }

    let started = Instant::now();
    let entries = glib::MainContext::new()
        .block_on(GioDirectoryReader::new().read_directory(&Location::Path(dir.path().into())))
        .unwrap();
    let listing = started.elapsed();
    assert_eq!(entries.len(), ENTRIES);

    let mut timings = Vec::new();
    for (label, options) in [
        ("sort by name (natural)", ViewOptions::default()),
        (
            "sort by modified, descending",
            ViewOptions {
                sort: SortOrder {
                    key: SortKey::Modified,
                    descending: true,
                },
                ..ViewOptions::default()
            },
        ),
        (
            "search \"file-99\"",
            ViewOptions {
                query: "file-99".into(),
                ..ViewOptions::default()
            },
        ),
    ] {
        let started = Instant::now();
        let shown = arrange(&entries, &options);
        timings.push((label, started.elapsed(), shown.len()));
    }

    println!("\n{ENTRIES} entries");
    println!("  list (GIO, metadata only)      {listing:>10.1?}");
    for (label, elapsed, shown) in timings {
        println!("  {label:<30} {elapsed:>10.1?}  ({shown} shown)");
    }
}

#[test]
#[ignore = "benchmark"]
fn copy_throughput_against_cp() {
    // tmpfs avoids btrfs reflinks, which would make copies instant.
    let base = std::path::Path::new("/dev/shm");
    if !base.is_dir() {
        eprintln!("no /dev/shm; skipping");
        return;
    }
    let dir = tempfile::tempdir_in(base).unwrap();
    let src = dir.path().join("big.bin");
    let size = 512 * 1024 * 1024usize;
    fs::write(&src, vec![7u8; size]).unwrap();

    let dst_cp = dir.path().join("cp");
    fs::create_dir(&dst_cp).unwrap();
    let started = Instant::now();
    assert!(
        std::process::Command::new("cp")
            .arg(&src)
            .arg(&dst_cp)
            .status()
            .unwrap()
            .success()
    );
    let cp = started.elapsed();

    let dst = dir.path().join("seshat");
    fs::create_dir(&dst).unwrap();
    let manager = OperationManager::new(Arc::new(LocalFileSystem::new()), 1);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    manager.subscribe(Box::new(move |e| {
        let _ = tx.lock().unwrap().send(e.clone());
    }));
    let started = Instant::now();
    manager
        .submit(OperationRequest::Copy {
            sources: vec![src.clone()],
            destination: dst.clone(),
        })
        .unwrap();
    let mut progress_events = 0;
    loop {
        let event = rx.recv_timeout(Duration::from_secs(60)).unwrap();
        if event.kind == OperationEventKind::Progress {
            progress_events += 1;
        }
        if event.operation.is_finished() {
            break;
        }
    }
    let seshat = started.elapsed();

    let mb = size as f64 / 1_048_576.0;
    println!("\ncopy {mb:.0} MiB on tmpfs");
    println!(
        "  cp       {cp:>9.1?}  {:>7.0} MiB/s",
        mb / cp.as_secs_f64()
    );
    println!(
        "  seshat   {seshat:>9.1?}  {:>7.0} MiB/s  ({progress_events} progress events)",
        mb / seshat.as_secs_f64()
    );
}
