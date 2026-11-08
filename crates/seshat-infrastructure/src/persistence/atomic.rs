use std::io::Write;
use std::path::Path;

/// Writes `contents` to `path` atomically: a crash leaves either the old or
/// the new file, never a truncated one.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent directory"))?;
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
