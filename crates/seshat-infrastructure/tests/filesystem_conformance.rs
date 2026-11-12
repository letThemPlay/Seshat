//! `LocalFileSystem` against the `FileSystem` contract, the same checks
//! the in-memory test double passes (`operations::conformance`). Each check
//! runs in its own temporary folder; the Trash is not touched.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use seshat_application::operations::FileSystem;
use seshat_application::operations::conformance::{self, Fixture};
use seshat_infrastructure::filesystem::LocalFileSystem;

struct LocalFixture {
    fs: LocalFileSystem,
    /// Removed when the fixture is dropped.
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture for LocalFixture {
    fn filesystem(&self) -> &dyn FileSystem {
        &self.fs
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write_file(&self, path: &Path, content: &[u8]) {
        fs::write(path, content).unwrap();
    }

    fn symlink(&self, path: &Path, target: &Path) {
        symlink(target, path).unwrap();
    }

    fn read_file(&self, path: &Path) -> Option<Vec<u8>> {
        let meta = fs::symlink_metadata(path).ok()?;
        meta.is_file().then(|| fs::read(path).unwrap())
    }
}

#[test]
fn local_filesystem_meets_the_filesystem_contract() {
    conformance::run_all(&|| {
        let dir = tempfile::tempdir().unwrap();
        // The temporary folder may be reached through a link (e.g. a
        // /tmp that is one); the checks compare canonical paths.
        let root = fs::canonicalize(dir.path()).unwrap();
        Box::new(LocalFixture {
            fs: LocalFileSystem::new(),
            _dir: dir,
            root,
        })
    });
}
