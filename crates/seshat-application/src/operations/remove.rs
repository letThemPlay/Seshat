//! Permanent deletion, moving to the Trash and restoring from it.

use std::path::{Path, PathBuf};

use seshat_domain::entities::Location;
use seshat_domain::operation::{Conflict, ConflictResolution, EntryType};

use super::filesystem::FsErrorKind;
use super::job::{Job, JobError, JobResult, scan};
use super::naming::unique_path;

pub(crate) fn delete(job: &mut Job<'_>, items: &[Location]) -> JobResult {
    let mut files = 0;
    for item in items {
        files += match item {
            Location::Path(path) => scan(job, path)?.0.max(1),
            Location::Trash(_) => 1,
        };
    }
    job.begin(files, None);
    for item in items {
        job.check_cancel()?;
        match item {
            Location::Path(path) => {
                delete_tree(job, path)?;
            }
            Location::Trash(rel) => {
                job.set_current(rel);
                match job.fs.delete_trashed(rel) {
                    Ok(()) => job.item_done(),
                    Err(err) => job.item_failed(rel, err.message),
                }
            }
        }
    }
    Ok(())
}

/// Deletes bottom-up without following symlinks. Returns whether
/// everything was deleted.
fn delete_tree(job: &mut Job<'_>, path: &Path) -> Result<bool, JobError> {
    job.check_cancel()?;
    job.set_current(path);
    let meta = match job.fs.metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind == FsErrorKind::NotFound => {
            job.item_done(); // already gone
            return Ok(true);
        }
        Err(err) => {
            job.item_failed(path, err.message);
            return Ok(false);
        }
    };
    if meta.entry_type == EntryType::Directory {
        let children = match job.fs.read_dir(path) {
            Ok(children) => children,
            Err(err) => {
                job.item_failed(path, err.message);
                return Ok(false);
            }
        };
        let mut all = true;
        for name in children {
            all &= delete_tree(job, &path.join(name))?;
        }
        if !all {
            return Ok(false); // keep the folder; its remaining contents failed
        }
        match job.fs.remove_empty_dir(path) {
            Ok(()) => job.item_done(),
            Err(err) => {
                job.item_failed(path, err.message);
                return Ok(false);
            }
        }
    } else {
        match job.fs.remove_file(path) {
            Ok(()) => job.item_done(),
            Err(err) => {
                job.item_failed(path, err.message);
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Moves items to the Trash. A failure is reported and the item is left
/// untouched; it is never deleted as a fallback.
pub(crate) fn trash(job: &mut Job<'_>, paths: &[PathBuf]) -> JobResult {
    job.begin(paths.len() as u64, None);
    for path in paths {
        job.check_cancel()?;
        job.set_current(path);
        match job.fs.trash(path) {
            Ok(()) => job.item_done(),
            Err(err) => job.item_failed(
                path,
                format!("Could not move to the Trash: {err}. The item was not deleted."),
            ),
        }
    }
    Ok(())
}

pub(crate) fn restore(job: &mut Job<'_>, items: &[PathBuf]) -> JobResult {
    job.begin(items.len() as u64, None);
    for item in items {
        job.check_cancel()?;
        job.set_current(item);
        let origin = match job.fs.trash_origin(item) {
            Ok(origin) => origin,
            Err(err) => {
                job.item_failed(item, format!("Original location unknown: {err}"));
                continue;
            }
        };
        let mut target = origin.clone();
        let mut replace = false;
        if let Ok(existing) = job.fs.metadata(&target) {
            let conflict = Conflict {
                source: PathBuf::from(format!("trash:///{}", item.display())),
                target: target.clone(),
                source_type: existing.entry_type,
                target_type: existing.entry_type,
            };
            match job.resolve(conflict)? {
                ConflictResolution::Skip => {
                    job.skipped(1, 0);
                    continue;
                }
                ConflictResolution::Rename => {
                    let is_dir = existing.entry_type == EntryType::Directory;
                    target = unique_path(&target, is_dir, |p| job.fs.exists(p));
                }
                ConflictResolution::Overwrite => replace = true,
                ConflictResolution::Cancel => return Err(JobError::Cancelled),
            }
        }
        if let Some(parent) = target.parent()
            && let Err(err) = create_missing_dirs(job, parent)
        {
            job.item_failed(
                item,
                format!("Could not recreate {}: {err}", parent.display()),
            );
            continue;
        }
        match job.fs.restore(item, &target, replace) {
            Ok(()) => job.item_done(),
            Err(err) => job.item_failed(item, err.message),
        }
    }
    Ok(())
}

/// Recreates a restored item's missing parent folders.
fn create_missing_dirs(job: &Job<'_>, dir: &Path) -> Result<(), super::FsError> {
    let mut missing = Vec::new();
    let mut current = Some(dir);
    while let Some(path) = current {
        if job.fs.exists(path) {
            break;
        }
        missing.push(path.to_path_buf());
        current = path.parent();
    }
    for path in missing.into_iter().rev() {
        match job.fs.create_dir(&path) {
            Ok(()) => {}
            Err(err) if err.kind == FsErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}
