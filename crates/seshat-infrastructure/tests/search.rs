//! Fuzzy Find against a real filesystem: the walker and the matcher behind
//! the search engine (Spec 4 §17 integration tests).
//!
//! The benchmark is opt-in:
//! `cargo test --release -p seshat-infrastructure --test search -- --ignored --nocapture`.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use seshat_application::operations::CancelToken;
use seshat_application::ports::{Candidate, TreeWalker, WalkError, WalkOptions, WalkStats};
use seshat_application::search::{
    DEFAULT_RESULT_LIMIT, SearchEngine, SearchRequest, SearchRoot, SearchSummary,
};
use seshat_domain::entities::EntryKind;
use seshat_domain::search::{SearchQuery, SearchResult, SearchStatus};
use seshat_infrastructure::search::{LocalTreeWalker, NucleoMatcher};

fn walk(root: &Path, options: &WalkOptions) -> (Result<WalkStats, WalkError>, Vec<Candidate>) {
    let mut found = Vec::new();
    let result = LocalTreeWalker.walk(root, options, &CancelToken::new(), &mut |c| found.push(c));
    (result, found)
}

fn relative(root: &Path, found: &[Candidate]) -> Vec<String> {
    let mut paths: Vec<String> = found
        .iter()
        .map(|c| {
            c.path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    paths.sort();
    paths
}

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::File::create(path).unwrap();
}

fn search(root: &Path, query: &str) -> (SearchSummary, Vec<SearchResult>) {
    let mut engine = SearchEngine::new(Arc::new(LocalTreeWalker), Arc::new(NucleoMatcher));
    let request = SearchRequest {
        roots: vec![SearchRoot {
            path: root.to_path_buf(),
            origin: None,
        }],
        query: SearchQuery::new(query),
        options: WalkOptions::default(),
        limit: DEFAULT_RESULT_LIMIT,
    };
    let mut last = Vec::new();
    let summary = engine.run(&request, &CancelToken::new(), &mut |r| last = r);
    (summary, last)
}

#[test]
fn walks_recursively_without_hidden_or_ignored_names() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(&root.join("a/b/c.txt"));
    touch(&root.join(".hidden/secret.txt"));
    touch(&root.join(".profile"));
    touch(&root.join("project/.git/config"));
    touch(&root.join("project/node_modules/pkg/index.js"));
    touch(&root.join("project/main.rs"));

    let (result, found) = walk(root, &WalkOptions::default());
    assert_eq!(result.unwrap(), WalkStats::default());
    assert_eq!(
        relative(root, &found),
        ["a", "a/b", "a/b/c.txt", "project", "project/main.rs"]
    );
    let kinds: Vec<EntryKind> = found
        .iter()
        .filter(|c| c.path.ends_with("a/b"))
        .map(|c| c.kind)
        .collect();
    assert_eq!(kinds, [EntryKind::Directory]);

    let options = WalkOptions {
        include_hidden: true,
        ignore_names: Vec::new(),
        ..WalkOptions::default()
    };
    let (_, found) = walk(root, &options);
    let all = relative(root, &found);
    for path in [".hidden/secret.txt", ".profile", "project/.git/config"] {
        assert!(all.contains(&path.to_owned()), "{path}");
    }
}

#[test]
fn symbolic_links_are_listed_but_never_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let outside = tempfile::tempdir().unwrap();
    touch(&outside.path().join("elsewhere.txt"));
    touch(&root.join("real/file.txt"));
    symlink(outside.path(), root.join("link-out")).unwrap();
    // A loop: a link back to the root.
    symlink(root, root.join("real/loop")).unwrap();
    symlink(root.join("missing"), root.join("broken")).unwrap();

    let (result, found) = walk(root, &WalkOptions::default());
    result.unwrap();
    assert_eq!(
        relative(root, &found),
        ["broken", "link-out", "real", "real/file.txt", "real/loop"]
    );
    let kind_of = |name: &str| found.iter().find(|c| c.path.ends_with(name)).unwrap().kind;
    assert_eq!(
        kind_of("link-out"),
        EntryKind::Symlink { to_directory: true }
    );
    assert_eq!(
        kind_of("broken"),
        EntryKind::Symlink {
            to_directory: false
        }
    );
}

#[test]
fn unreadable_folders_are_skipped_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(&root.join("open/visible.txt"));
    touch(&root.join("locked/inside.txt"));
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();

    let (result, found) = walk(root, &WalkOptions::default());
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o700)).unwrap();
    let stats = result.unwrap();
    assert_eq!(stats.unreadable, 1);
    assert_eq!(
        relative(root, &found),
        ["locked", "open", "open/visible.txt"]
    );
}

#[test]
fn the_root_must_be_a_readable_folder() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    assert_eq!(
        walk(&missing, &WalkOptions::default()).0,
        Err(WalkError::NotFound(missing.clone()))
    );
    let file = dir.path().join("file.txt");
    touch(&file);
    assert_eq!(
        walk(&file, &WalkOptions::default()).0,
        Err(WalkError::NotAFolder(file.clone()))
    );
    // A link to a folder is fine as a root: a panel can show one.
    let target = dir.path().join("target");
    touch(&target.join("x.txt"));
    symlink(&target, dir.path().join("via-link")).unwrap();
    let (result, found) = walk(&dir.path().join("via-link"), &WalkOptions::default());
    result.unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn non_utf8_and_unicode_names_keep_their_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let raw = OsStr::from_bytes(b"bad\xffname.txt");
    touch(&root.join(raw));
    touch(&root.join("Ünïcødé ✓.txt"));

    let (_, found) = walk(root, &WalkOptions::default());
    assert!(found.iter().any(|c| c.path.file_name() == Some(raw)));

    let (_, results) = search(root, "badname");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].location,
        seshat_domain::entities::Location::Path(root.join(raw)),
        "the native path is kept; only the display name is lossy"
    );
    let (_, results) = search(root, "unicode");
    assert_eq!(results[0].name, "Ünïcødé ✓.txt");
    let name = &results[0].name;
    for r in &results[0].name_match.ranges {
        assert!(name.is_char_boundary(r.start) && name.is_char_boundary(r.end));
    }
}

#[test]
fn the_entry_limit_bounds_a_walk() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..20 {
        touch(&dir.path().join(format!("f{i}")));
    }
    let options = WalkOptions {
        max_entries: 5,
        ..WalkOptions::default()
    };
    let (result, found) = walk(dir.path(), &options);
    assert!(result.unwrap().truncated);
    assert_eq!(found.len(), 5);
}

#[test]
fn folders_removed_during_a_walk_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for name in ["a", "b", "c"] {
        touch(&root.join(name).join("inner/deep.txt"));
    }
    // Delete every other folder as soon as the first entry is reported.
    let mut deleted = false;
    let mut found: Vec<PathBuf> = Vec::new();
    let result = LocalTreeWalker.walk(
        &root,
        &WalkOptions::default(),
        &CancelToken::new(),
        &mut |c| {
            if !deleted {
                deleted = true;
                for name in ["a", "b", "c"] {
                    if !c.path.starts_with(root.join(name)) {
                        let _ = fs::remove_dir_all(root.join(name));
                    }
                }
            }
            found.push(c.path);
        },
    );
    let stats = result.unwrap();
    assert!(!found.is_empty());
    assert!(
        stats.unreadable <= 2,
        "vanished folders are skipped, not fatal"
    );
}

#[test]
fn cancelling_stops_the_walk_promptly() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..1000 {
        touch(&dir.path().join(format!("d{}/f{i}", i % 10)));
    }
    let cancel = CancelToken::new();
    let mut seen = 0;
    LocalTreeWalker
        .walk(dir.path(), &WalkOptions::default(), &cancel, &mut |_| {
            seen += 1;
            if seen == 10 {
                cancel.cancel();
            }
        })
        .unwrap();
    assert_eq!(seen, 10);
}

#[test]
fn finds_the_specs_example_with_the_real_matcher() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("Documents")).unwrap();
    fs::create_dir_all(root.join("Development/Code/Modules")).unwrap();
    fs::create_dir_all(root.join("Downloads/Camera")).unwrap();
    touch(&root.join("notes.txt"));

    let (summary, results) = search(root, "dcm");
    assert_eq!(summary.status, SearchStatus::Completed);
    let paths: Vec<&str> = results.iter().map(|r| r.relative_path.as_str()).collect();
    for expected in ["Documents", "Development/Code/Modules", "Downloads/Camera"] {
        assert!(paths.contains(&expected), "{expected} in {paths:?}");
    }
    assert!(!paths.contains(&"notes.txt"));
    // A name match ranks above path-only matches.
    assert_eq!(paths[0], "Documents");
}

/// Spec 4 §11: first results in about 100 ms for small folders; this
/// measures a 100,000-file tree. Prints timings; asserts only correctness.
#[test]
#[ignore = "benchmark"]
fn large_tree_search_timings() {
    let dir = tempfile::tempdir().unwrap();
    for d in 0..100 {
        let folder = dir.path().join(format!("folder-{d:03}"));
        fs::create_dir(&folder).unwrap();
        for f in 0..1000 {
            fs::File::create(folder.join(format!("file-{f:04}.txt"))).unwrap();
        }
    }
    let mut engine = SearchEngine::new(Arc::new(LocalTreeWalker), Arc::new(NucleoMatcher));
    let mut request = SearchRequest {
        roots: vec![SearchRoot {
            path: dir.path().to_path_buf(),
            origin: None,
        }],
        query: SearchQuery::new("f042 0999"),
        options: WalkOptions::default(),
        limit: DEFAULT_RESULT_LIMIT,
    };

    let started = Instant::now();
    let mut first = None;
    let mut last = Vec::new();
    let summary = engine.run(&request, &CancelToken::new(), &mut |r| {
        first.get_or_insert_with(|| started.elapsed());
        last = r;
    });
    let walk_and_rank = started.elapsed();
    assert_eq!(summary.candidates, 100_100);
    assert!(!last.is_empty());

    request.query = SearchQuery::new("fold 5 file 12");
    let started = Instant::now();
    engine.run(&request, &CancelToken::new(), &mut |r| last = r);
    let rerank = started.elapsed();

    println!("first results while walking: {:?}", first.unwrap());
    println!("walk + rank 100,100 entries: {walk_and_rank:?}");
    println!("new query on cached entries: {rerank:?}");
}
