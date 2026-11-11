//! Walks local folder trees for Fuzzy Find with `std::fs` (Spec 4 §12).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use seshat_application::operations::CancelToken;
use seshat_application::ports::{Candidate, TreeWalker, WalkError, WalkOptions, WalkStats};
use seshat_domain::entities::EntryKind;

/// Blocking; runs on the search worker thread.
///
/// - Symbolic links are listed but never followed, so links cannot lead
///   the walk elsewhere or into a loop. The root itself may be a link to a
///   folder (a panel can show one).
/// - Every folder is entered at most once, by device and inode, which also
///   guards against bind-mount loops.
/// - With `same_device`, mount points are listed but not entered.
/// - Folders that can't be read, or vanish during the walk, are skipped
///   and counted; the walk goes on.
/// - Names are kept as native paths; nothing assumes UTF-8.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalTreeWalker;

impl LocalTreeWalker {
    pub fn new() -> Self {
        Self
    }
}

fn root_error(root: &Path, err: &io::Error) -> WalkError {
    let root = root.to_path_buf();
    match err.kind() {
        io::ErrorKind::NotFound => WalkError::NotFound(root),
        io::ErrorKind::PermissionDenied => WalkError::PermissionDenied(root),
        io::ErrorKind::NotADirectory => WalkError::NotAFolder(root),
        _ => WalkError::Other(root, err.to_string()),
    }
}

impl TreeWalker for LocalTreeWalker {
    fn walk(
        &self,
        root: &Path,
        options: &WalkOptions,
        cancel: &CancelToken,
        found: &mut dyn FnMut(Candidate),
    ) -> Result<WalkStats, WalkError> {
        let meta = fs::metadata(root).map_err(|e| root_error(root, &e))?;
        if !meta.is_dir() {
            return Err(WalkError::NotAFolder(root.to_path_buf()));
        }
        // Reading the root is the one failure that ends the walk.
        let first = fs::read_dir(root).map_err(|e| root_error(root, &e))?;
        let device = meta.dev();
        let mut visited = HashSet::from([(meta.dev(), meta.ino())]);
        let mut stats = WalkStats::default();
        let mut emitted = 0;
        let mut pending: Vec<PathBuf> = Vec::new();
        let mut current = Some(first);

        while let Some(entries) = current.take() {
            for entry in entries {
                if cancel.is_cancelled() {
                    return Ok(stats);
                }
                // An entry that vanished between listing and reading.
                let Ok(entry) = entry else { continue };
                let name = entry.file_name();
                if !options.include_hidden && name.as_encoded_bytes().first() == Some(&b'.') {
                    continue;
                }
                if options.ignore_names.contains(&name) {
                    continue;
                }
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if emitted == options.max_entries {
                    stats.truncated = true;
                    return Ok(stats);
                }
                let path = entry.path();
                let kind = if file_type.is_symlink() {
                    EntryKind::Symlink {
                        to_directory: fs::metadata(&path).is_ok_and(|m| m.is_dir()),
                    }
                } else if file_type.is_dir() {
                    EntryKind::Directory
                } else if file_type.is_file() {
                    EntryKind::File
                } else {
                    EntryKind::Other
                };
                if kind == EntryKind::Directory
                    && let Ok(meta) = entry.metadata()
                    && (!options.same_device || meta.dev() == device)
                    && visited.insert((meta.dev(), meta.ino()))
                {
                    pending.push(path.clone());
                }
                found(Candidate { path, kind });
                emitted += 1;
            }
            while let Some(dir) = pending.pop() {
                match fs::read_dir(&dir) {
                    Ok(entries) => {
                        current = Some(entries);
                        break;
                    }
                    Err(err) => {
                        tracing::debug!(error = %err, "search skipped an unreadable folder");
                        stats.unreadable += 1;
                    }
                }
            }
        }
        Ok(stats)
    }
}
