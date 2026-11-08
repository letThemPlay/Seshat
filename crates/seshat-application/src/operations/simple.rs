//! Single-item operations: rename and create. Their errors fail the
//! operation with a specific message.

use std::path::Path;

use super::filesystem::{FsError, FsErrorKind};
use super::job::{Job, JobError, JobResult};

fn describe(err: FsError, path: &Path) -> JobError {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    JobError::Fatal(match err.kind {
        FsErrorKind::AlreadyExists => format!("“{name}” already exists."),
        FsErrorKind::PermissionDenied => "You don't have permission to do that here.".to_owned(),
        FsErrorKind::ReadOnly => "The location is read-only.".to_owned(),
        _ => err.message,
    })
}

pub(crate) fn rename(job: &mut Job<'_>, path: &Path, new_name: &str) -> JobResult {
    job.begin(1, None);
    job.set_current(path);
    let target = path.with_file_name(new_name);
    if target == path {
        job.item_done();
        return Ok(());
    }
    job.fs
        .rename(path, &target, false)
        .map_err(|err| describe(err, &target))?;
    job.item_done();
    Ok(())
}

pub(crate) fn create(job: &mut Job<'_>, path: &Path, directory: bool) -> JobResult {
    job.begin(1, None);
    job.set_current(path);
    let result = if directory {
        job.fs.create_dir(path)
    } else {
        job.fs.create_file(path)
    };
    result.map_err(|err| describe(err, path))?;
    job.item_done();
    Ok(())
}
