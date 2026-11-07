//! Copy and move.
//!
//! Safety rules:
//! - Replacing an existing file copies to a hidden temporary sibling first
//!   and renames it over the target only when complete, so a failed or
//!   cancelled copy never destroys the original.
//! - A moved item's source is removed only after it was copied completely.
//! - Partial files we created are removed on failure; files we did not
//!   create are never touched.

use std::path::Path;

use seshat_domain::operation::{Conflict, ConflictResolution, EntryType};

use super::filesystem::{FsErrorKind, FsMetadata};
use super::job::{Job, JobError, JobResult, scan};
use super::naming::{temp_sibling, unique_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Copy,
    Move,
}

pub(crate) fn run(
    job: &mut Job<'_>,
    sources: &[std::path::PathBuf],
    destination: &Path,
    mode: Mode,
) -> JobResult {
    // Destinations are often reached through symlinks (e.g. ~/Projects ->
    // /data/projects): work on the real folder.
    let destination = &job
        .fs
        .canonicalize(destination)
        .map_err(|e| JobError::Fatal(format!("The destination is not available: {e}")))?;
    let dest = job
        .fs
        .metadata(destination)
        .map_err(|e| JobError::Fatal(format!("The destination is not available: {e}")))?;
    if dest.entry_type != EntryType::Directory {
        return Err(JobError::Fatal(
            "The destination is not a folder.".to_owned(),
        ));
    }

    // The request was checked lexically; aliases (a symlinked destination
    // inside a source folder) only show up on the real paths. Copying a
    // folder into itself would recurse until the disk is full.
    for source in sources {
        if job
            .fs
            .metadata(source)
            .is_ok_and(|m| m.entry_type == EntryType::Directory)
            && let Ok(real) = job.fs.canonicalize(source)
            && destination.starts_with(&real)
        {
            return Err(JobError::Fatal(format!(
                "Cannot {} “{}” into itself.",
                if mode == Mode::Copy { "copy" } else { "move" },
                source
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            )));
        }
    }

    // Plan: same-device moves are single renames; everything else is
    // scanned so progress and free space can be computed up front.
    let (mut files, mut bytes) = (0u64, 0u64);
    for source in sources {
        job.check_cancel()?;
        match job.fs.metadata(source) {
            Ok(meta) if mode == Mode::Move && meta.device == dest.device => files += 1,
            Ok(_) => {
                let (f, b) = scan(job, source)?;
                files += f;
                bytes += b;
            }
            Err(_) => files += 1, // reported when processed
        }
    }
    if bytes > 0
        && let Some(available) = job.fs.available_space(destination)
        && bytes > available
    {
        return Err(JobError::Fatal(format!(
            "Not enough free space: {} needed, {} available.",
            human_size(bytes),
            human_size(available)
        )));
    }
    job.begin(files, Some(bytes));

    for source in sources {
        job.check_cancel()?;
        let Some(name) = source.file_name() else {
            continue;
        };
        transfer(job, source, &destination.join(name), mode, dest.device)?;
    }
    Ok(())
}

/// Transfers one item, resolving a conflict at `target` if needed.
/// Returns whether the item was transferred completely.
fn transfer(
    job: &mut Job<'_>,
    source: &Path,
    target: &Path,
    mode: Mode,
    dest_device: u64,
) -> Result<bool, JobError> {
    job.check_cancel()?;
    job.set_current(source);
    let meta = match job.fs.metadata(source) {
        Ok(meta) => meta,
        Err(err) => {
            job.item_failed(source, format!("Could not read the item: {err}"));
            return Ok(false);
        }
    };
    let is_dir = meta.entry_type == EntryType::Directory;
    let mut target = target.to_path_buf();
    let mut replace = false;

    if source == target {
        match mode {
            // Copying into the same folder makes a duplicate.
            Mode::Copy => target = unique_path(&target, is_dir, |p| job.fs.exists(p)),
            // Moving onto itself: nothing to do.
            Mode::Move => {
                job.item_done();
                return Ok(true);
            }
        }
    } else if let Ok(existing) = job.fs.metadata(&target) {
        let conflict = Conflict {
            source: source.to_path_buf(),
            target: target.clone(),
            source_type: meta.entry_type,
            target_type: existing.entry_type,
        };
        match job.resolve(conflict)? {
            ConflictResolution::Skip => {
                let (files, bytes) = if mode == Mode::Move && meta.device == dest_device {
                    (1, 0)
                } else {
                    scan(job, source)?
                };
                job.skipped(files, bytes);
                return Ok(false);
            }
            ConflictResolution::Rename => {
                target = unique_path(&target, is_dir, |p| job.fs.exists(p));
            }
            ConflictResolution::Overwrite => {
                let target_is_dir = existing.entry_type == EntryType::Directory;
                if is_dir && target_is_dir {
                    // Merge folders.
                    return copy_tree(job, source, &target, meta, false, mode, dest_device);
                }
                if is_dir != target_is_dir {
                    job.item_failed(
                        source,
                        if is_dir {
                            "A file with this name exists; a folder cannot replace it."
                        } else {
                            "A folder with this name exists; a file cannot replace it."
                        },
                    );
                    return Ok(false);
                }
                replace = true;
            }
            ConflictResolution::Cancel => return Err(JobError::Cancelled),
        }
    }

    if mode == Mode::Move && meta.device == dest_device {
        match job.fs.rename(source, &target, replace) {
            Ok(()) => {
                job.item_done();
                return Ok(true);
            }
            // e.g. bind mounts of one device: fall back to copy + delete
            Err(err) if err.kind == FsErrorKind::CrossDevice => {}
            Err(err) => {
                job.item_failed(source, err.message);
                return Ok(false);
            }
        }
    }
    copy_tree(job, source, &target, meta, replace, mode, dest_device)
}

/// Copies `source` to `target` (recursively); with `Mode::Move` each item's
/// source is removed once it has been copied completely.
fn copy_tree(
    job: &mut Job<'_>,
    source: &Path,
    target: &Path,
    meta: FsMetadata,
    replace: bool,
    mode: Mode,
    dest_device: u64,
) -> Result<bool, JobError> {
    let complete = match meta.entry_type {
        EntryType::File => copy_file_safely(job, source, target, replace)?,
        EntryType::Symlink => copy_symlink_safely(job, source, target, replace)?,
        EntryType::Directory => {
            if !job.fs.exists(target)
                && let Err(err) = job.fs.create_dir(target)
            {
                job.item_failed(source, format!("Could not create the folder: {err}"));
                return Ok(false);
            }
            job.item_done();
            let children = match job.fs.read_dir(source) {
                Ok(children) => children,
                Err(err) => {
                    job.item_failed(source, format!("Could not read the folder: {err}"));
                    return Ok(false);
                }
            };
            let mut all = true;
            for name in children {
                all &= transfer(
                    job,
                    &source.join(&name),
                    &target.join(&name),
                    mode,
                    dest_device,
                )?;
            }
            if let Err(err) = job.fs.copy_dir_attributes(source, target) {
                tracing::debug!(path = %target.display(), error = %err, "folder attributes not copied");
            }
            if mode == Mode::Move
                && all
                && let Err(err) = job.fs.remove_empty_dir(source)
            {
                job.item_failed(
                    source,
                    format!("Moved, but the original folder could not be removed: {err}"),
                );
                return Ok(false);
            }
            return Ok(all);
        }
        EntryType::Other => {
            job.item_failed(
                source,
                "Special files (devices, sockets, pipes) are not copied.",
            );
            return Ok(false);
        }
    };
    if !complete {
        return Ok(false);
    }
    if mode == Mode::Move
        && let Err(err) = job.fs.remove_file(source)
    {
        job.item_failed(
            source,
            format!("Copied, but the original could not be removed: {err}"),
        );
        return Ok(false);
    }
    job.item_done();
    Ok(true)
}

fn copy_file_safely(
    job: &mut Job<'_>,
    source: &Path,
    target: &Path,
    replace: bool,
) -> Result<bool, JobError> {
    let dest = if replace {
        temp_sibling(target, &job.token)
    } else {
        target.to_path_buf()
    };
    let fs = job.fs.clone();
    let result = {
        let job_ref: &Job<'_> = job;
        fs.copy_file(source, &dest, &mut |bytes| {
            job_ref.add_bytes(bytes);
            !job_ref.cancelled()
        })
    };
    match result {
        Ok(()) if replace => match fs.rename(&dest, target, true) {
            Ok(()) => Ok(true),
            Err(err) => {
                let _ = fs.remove_file(&dest);
                job.item_failed(
                    source,
                    format!("Could not replace the existing file: {err}"),
                );
                Ok(false)
            }
        },
        Ok(()) => Ok(true),
        Err(err) => {
            // Never remove something we did not create.
            if err.kind != FsErrorKind::AlreadyExists {
                let _ = fs.remove_file(&dest);
            }
            match err.kind {
                FsErrorKind::Cancelled => Err(JobError::Cancelled),
                FsErrorKind::NoSpace => Err(JobError::Fatal("The destination is full.".to_owned())),
                _ => {
                    job.item_failed(source, err.message);
                    Ok(false)
                }
            }
        }
    }
}

fn copy_symlink_safely(
    job: &mut Job<'_>,
    source: &Path,
    target: &Path,
    replace: bool,
) -> Result<bool, JobError> {
    let fs = job.fs.clone();
    let dest = if replace {
        temp_sibling(target, &job.token)
    } else {
        target.to_path_buf()
    };
    let result = fs.copy_symlink(source, &dest).and_then(|()| {
        if replace {
            fs.rename(&dest, target, true).inspect_err(|_| {
                let _ = fs.remove_file(&dest);
            })
        } else {
            Ok(())
        }
    });
    match result {
        Ok(()) => Ok(true),
        Err(err) => {
            job.item_failed(source, err.message);
            Ok(false)
        }
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::human_size;

    #[test]
    fn sizes() {
        assert_eq!(human_size(12), "12 bytes");
        assert_eq!(human_size(1_500_000), "1.5 MB");
    }
}
