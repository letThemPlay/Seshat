//! In-memory [`FileSystem`] for testing the manager and algorithms.
//!
//! Paths under `/other` are on a second device (for cross-device moves).

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use seshat_domain::operation::EntryType;

use super::filesystem::{FileSystem, FsError, FsErrorKind, FsMetadata};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    File(Vec<u8>),
    Dir,
    Link(PathBuf),
}

#[derive(Default)]
struct TrashItem {
    origin: PathBuf,
    nodes: Vec<(PathBuf, Node)>,
}

#[derive(Default)]
pub struct MemoryFs {
    nodes: Mutex<BTreeMap<PathBuf, Node>>,
    trash: Mutex<BTreeMap<PathBuf, TrashItem>>,
    pub free_space: Mutex<Option<u64>>,
    pub trash_unsupported: Mutex<bool>,
    /// Copies of files with these names fail half-way.
    pub fail_copies_of: Mutex<HashSet<OsString>>,
    /// While closed, file copies block (still honouring cancellation).
    gate: (Mutex<bool>, Condvar),
}

fn err(kind: FsErrorKind, message: &str) -> FsError {
    FsError::new(kind, message)
}

fn device(path: &Path) -> u64 {
    if path.starts_with("/other") { 2 } else { 1 }
}

impl MemoryFs {
    pub fn new() -> Self {
        let fs = Self::default();
        *fs.gate.0.lock().unwrap() = true;
        fs.dir("/");
        fs.dir("/other");
        fs
    }

    pub fn dir(&self, path: &str) {
        self.nodes.lock().unwrap().insert(path.into(), Node::Dir);
    }

    pub fn file(&self, path: &str, content: &str) {
        self.nodes
            .lock()
            .unwrap()
            .insert(path.into(), Node::File(content.as_bytes().to_vec()));
    }

    pub fn link(&self, path: &str, target: &str) {
        self.nodes
            .lock()
            .unwrap()
            .insert(path.into(), Node::Link(target.into()));
    }

    pub fn get(&self, path: &str) -> Option<Node> {
        self.nodes.lock().unwrap().get(Path::new(path)).cloned()
    }

    pub fn content(&self, path: &str) -> Option<String> {
        match self.get(path)? {
            Node::File(bytes) => Some(String::from_utf8(bytes).unwrap()),
            _ => None,
        }
    }

    pub fn paths(&self) -> Vec<String> {
        self.nodes
            .lock()
            .unwrap()
            .keys()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    pub fn trashed(&self) -> Vec<String> {
        self.trash
            .lock()
            .unwrap()
            .keys()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    pub fn set_gate(&self, open: bool) {
        *self.gate.0.lock().unwrap() = open;
        self.gate.1.notify_all();
    }

    fn parent_is_dir(nodes: &BTreeMap<PathBuf, Node>, path: &Path) -> Result<(), FsError> {
        match path.parent().and_then(|p| nodes.get(p)) {
            Some(Node::Dir) => Ok(()),
            _ => Err(err(FsErrorKind::NotFound, "parent folder does not exist")),
        }
    }

    fn subtree(nodes: &BTreeMap<PathBuf, Node>, root: &Path) -> Vec<PathBuf> {
        nodes
            .keys()
            .filter(|p| p.starts_with(root))
            .cloned()
            .collect()
    }
}

impl FileSystem for MemoryFs {
    fn metadata(&self, path: &Path) -> Result<FsMetadata, FsError> {
        let nodes = self.nodes.lock().unwrap();
        let node = nodes
            .get(path)
            .ok_or_else(|| err(FsErrorKind::NotFound, "no such file"))?;
        let (entry_type, size) = match node {
            Node::File(bytes) => (EntryType::File, bytes.len() as u64),
            Node::Dir => (EntryType::Directory, 0),
            Node::Link(_) => (EntryType::Symlink, 0),
        };
        Ok(FsMetadata {
            entry_type,
            size,
            device: device(path),
        })
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<OsString>, FsError> {
        let nodes = self.nodes.lock().unwrap();
        match nodes.get(path) {
            Some(Node::Dir) => Ok(nodes
                .keys()
                .filter(|p| p.parent() == Some(path) && p.as_path() != path)
                .filter_map(|p| p.file_name().map(OsString::from))
                .collect()),
            Some(_) => Err(err(FsErrorKind::Other, "not a directory")),
            None => Err(err(FsErrorKind::NotFound, "no such folder")),
        }
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        // Resolves links in any component (absolute link targets only),
        // and `.` and `..` against what is resolved so far, like realpath.
        let nodes = self.nodes.lock().unwrap();
        let mut real = PathBuf::from("/");
        for component in path.components().skip(1) {
            match component {
                Component::CurDir => continue,
                Component::ParentDir => {
                    real.pop();
                    continue;
                }
                _ => real.push(component),
            }
            if let Some(Node::Link(target)) = nodes.get(&real) {
                real = target.clone();
            }
            if !nodes.contains_key(&real) {
                return Err(err(FsErrorKind::NotFound, "no such file"));
            }
        }
        Ok(real)
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        let mut nodes = self.nodes.lock().unwrap();
        if nodes.contains_key(path) {
            return Err(err(FsErrorKind::AlreadyExists, "exists"));
        }
        Self::parent_is_dir(&nodes, path)?;
        nodes.insert(path.to_path_buf(), Node::Dir);
        Ok(())
    }

    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        let mut nodes = self.nodes.lock().unwrap();
        if nodes.contains_key(path) {
            return Err(err(FsErrorKind::AlreadyExists, "exists"));
        }
        Self::parent_is_dir(&nodes, path)?;
        nodes.insert(path.to_path_buf(), Node::File(Vec::new()));
        Ok(())
    }

    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        on_progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), FsError> {
        let content = match self.nodes.lock().unwrap().get(from) {
            Some(Node::File(bytes)) => bytes.clone(),
            Some(_) => return Err(err(FsErrorKind::Other, "not a file")),
            None => return Err(err(FsErrorKind::NotFound, "source vanished")),
        };
        {
            let mut nodes = self.nodes.lock().unwrap();
            if nodes.contains_key(to) {
                return Err(err(FsErrorKind::AlreadyExists, "exists"));
            }
            Self::parent_is_dir(&nodes, to)?;
            nodes.insert(to.to_path_buf(), Node::File(Vec::new()));
        }
        // Wait at the gate, polling for cancellation.
        loop {
            let open = self.gate.0.lock().unwrap();
            if *open {
                break;
            }
            let _ = self
                .gate
                .1
                .wait_timeout(open, Duration::from_millis(5))
                .unwrap();
            if !on_progress(0) {
                return Err(err(FsErrorKind::Cancelled, "cancelled"));
            }
        }
        let fail = from
            .file_name()
            .is_some_and(|n| self.fail_copies_of.lock().unwrap().contains(n));
        for (i, chunk) in content.chunks(4).enumerate() {
            if fail && i * 4 >= content.len() / 2 {
                return Err(err(FsErrorKind::Other, "I/O error"));
            }
            if let Some(Node::File(bytes)) = self.nodes.lock().unwrap().get_mut(to) {
                bytes.extend_from_slice(chunk);
            }
            if !on_progress(chunk.len() as u64) {
                return Err(err(FsErrorKind::Cancelled, "cancelled"));
            }
        }
        Ok(())
    }

    fn copy_symlink(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let mut nodes = self.nodes.lock().unwrap();
        let link = nodes
            .get(from)
            .cloned()
            .ok_or_else(|| err(FsErrorKind::NotFound, "gone"))?;
        if nodes.contains_key(to) {
            return Err(err(FsErrorKind::AlreadyExists, "exists"));
        }
        nodes.insert(to.to_path_buf(), link);
        Ok(())
    }

    fn copy_dir_attributes(&self, _: &Path, _: &Path) -> Result<(), FsError> {
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> Result<(), FsError> {
        if device(from) != device(to) {
            return Err(err(FsErrorKind::CrossDevice, "cross-device"));
        }
        let mut nodes = self.nodes.lock().unwrap();
        if !nodes.contains_key(from) {
            return Err(err(FsErrorKind::NotFound, "gone"));
        }
        if let Some(existing) = nodes.get(to) {
            if !replace {
                return Err(err(FsErrorKind::AlreadyExists, "exists"));
            }
            if *existing == Node::Dir && Self::subtree(&nodes, to).len() > 1 {
                return Err(err(FsErrorKind::NotEmpty, "not empty"));
            }
            nodes.remove(to);
        }
        for old in Self::subtree(&nodes, from) {
            let node = nodes.remove(&old).unwrap();
            let new = to.join(old.strip_prefix(from).unwrap());
            nodes.insert(
                if new.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    new
                },
                node,
            );
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        let mut nodes = self.nodes.lock().unwrap();
        match nodes.get(path) {
            Some(Node::Dir) => Err(err(FsErrorKind::Other, "is a directory")),
            Some(_) => {
                nodes.remove(path);
                Ok(())
            }
            None => Err(err(FsErrorKind::NotFound, "gone")),
        }
    }

    fn remove_empty_dir(&self, path: &Path) -> Result<(), FsError> {
        let mut nodes = self.nodes.lock().unwrap();
        if Self::subtree(&nodes, path).len() > 1 {
            return Err(err(FsErrorKind::NotEmpty, "not empty"));
        }
        nodes
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| err(FsErrorKind::NotFound, "gone"))
    }

    fn available_space(&self, _: &Path) -> Option<u64> {
        *self.free_space.lock().unwrap()
    }

    fn trash(&self, path: &Path) -> Result<(), FsError> {
        if *self.trash_unsupported.lock().unwrap() {
            return Err(err(FsErrorKind::NotSupported, "Trash not supported here"));
        }
        let mut nodes = self.nodes.lock().unwrap();
        let name = PathBuf::from(path.file_name().unwrap());
        let mut item = TrashItem {
            origin: path.to_path_buf(),
            nodes: Vec::new(),
        };
        for old in Self::subtree(&nodes, path) {
            let node = nodes.remove(&old).unwrap();
            item.nodes
                .push((old.strip_prefix(path).unwrap().to_path_buf(), node));
        }
        if item.nodes.is_empty() {
            return Err(err(FsErrorKind::NotFound, "gone"));
        }
        self.trash.lock().unwrap().insert(name, item);
        Ok(())
    }

    fn trash_origin(&self, item: &Path) -> Result<PathBuf, FsError> {
        self.trash
            .lock()
            .unwrap()
            .get(item)
            .map(|i| i.origin.clone())
            .ok_or_else(|| err(FsErrorKind::NotFound, "not in trash"))
    }

    fn restore(&self, item: &Path, to: &Path, replace: bool) -> Result<(), FsError> {
        let mut trash = self.trash.lock().unwrap();
        let mut nodes = self.nodes.lock().unwrap();
        if nodes.contains_key(to) && !replace {
            return Err(err(FsErrorKind::AlreadyExists, "exists"));
        }
        Self::parent_is_dir(&nodes, to)?;
        let entry = trash
            .remove(item)
            .ok_or_else(|| err(FsErrorKind::NotFound, "not in trash"))?;
        for (rel, node) in entry.nodes {
            nodes.insert(
                if rel.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rel)
                },
                node,
            );
        }
        Ok(())
    }

    fn delete_trashed(&self, item: &Path) -> Result<(), FsError> {
        self.trash
            .lock()
            .unwrap()
            .remove(item)
            .map(|_| ())
            .ok_or_else(|| err(FsErrorKind::NotFound, "not in trash"))
    }
}

#[cfg(test)]
mod conformance_tests {
    use std::path::{Path, PathBuf};

    use super::{FileSystem, MemoryFs, Node};
    use crate::operations::conformance::{self, Fixture};

    struct MemoryFixture {
        fs: MemoryFs,
        root: PathBuf,
    }

    fn text(path: &Path) -> &str {
        path.to_str().expect("test paths are UTF-8")
    }

    impl Fixture for MemoryFixture {
        fn filesystem(&self) -> &dyn FileSystem {
            &self.fs
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn write_file(&self, path: &Path, content: &[u8]) {
            self.fs
                .nodes
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), Node::File(content.to_vec()));
        }

        fn symlink(&self, path: &Path, target: &Path) {
            self.fs.link(text(path), text(target));
        }

        fn read_file(&self, path: &Path) -> Option<Vec<u8>> {
            match self.fs.get(text(path))? {
                Node::File(bytes) => Some(bytes),
                _ => None,
            }
        }
    }

    /// The test double keeps the contract the real filesystem keeps, so
    /// tests written against it mean something.
    #[test]
    fn memory_fs_meets_the_filesystem_contract() {
        conformance::run_all(&|| {
            let fs = MemoryFs::new();
            fs.dir("/work");
            Box::new(MemoryFixture {
                fs,
                root: PathBuf::from("/work"),
            })
        });
    }
}
