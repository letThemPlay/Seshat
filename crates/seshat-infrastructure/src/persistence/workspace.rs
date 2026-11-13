//! The workspace between sessions: `$XDG_STATE_HOME/seshat/workspace.toml`.
//!
//! The format is versioned ([`WORKSPACE_SCHEMA_VERSION`]). Older versions
//! are migrated; newer ones are ignored rather than misread. Paths are
//! stored losslessly: as strings when valid UTF-8, otherwise as bytes.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};

use serde::{Deserialize, Serialize};
use seshat_application::workspace::{WORKSPACE_SCHEMA_VERSION, WorkspaceStore};
use seshat_domain::entities::{DirectoryIdentity, Location};
use seshat_domain::view::{SortKey, SortOrder, ViewMode, ViewOptions};
use seshat_domain::workspace::{
    BranchId, BranchParts, PanelId, PanelParts, PanelState, ScrollAnchor, Selection, Workspace,
    WorkspaceParts,
};

use super::atomic::write_atomic;

/// Larger files are not read: a workspace this big is not one we wrote.
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Saves the workspace as TOML. Encoding runs on the caller's thread;
/// writing, whose `fsync` can take tens of milliseconds, runs on a writer
/// thread. Saves queued during a write are coalesced to the latest.
#[derive(Debug)]
pub struct TomlWorkspaceStore {
    path: PathBuf,
    writer: Mutex<Option<mpsc::Sender<Job>>>,
}

#[derive(Debug)]
enum Job {
    Write(String),
    /// Answer once everything queued before it is written.
    Flush(mpsc::SyncSender<()>),
}

impl TomlWorkspaceStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            writer: Mutex::new(None),
        }
    }

    /// The writer thread's queue, starting the thread on first use. `None`
    /// if the thread cannot be started (then saves are written directly).
    fn writer(&self) -> Option<mpsc::Sender<Job>> {
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        if writer.is_none() {
            let (jobs, queue) = mpsc::channel();
            let path = self.path.clone();
            let started = std::thread::Builder::new()
                .name("seshat-workspace-writer".into())
                .spawn(move || write_queue(&path, &queue));
            match started {
                Ok(_) => *writer = Some(jobs),
                Err(err) => {
                    tracing::warn!(error = %err, "no workspace writer thread; saving directly")
                }
            }
        }
        writer.clone()
    }

    /// `$XDG_STATE_HOME/seshat/workspace.toml`.
    pub fn default_location() -> Self {
        Self::new(glib::user_state_dir().join("seshat").join("workspace.toml"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl WorkspaceStore for TomlWorkspaceStore {
    fn load(&self) -> Option<Workspace> {
        self.flush(); // never read a file that is about to be replaced
        let size = match std::fs::metadata(&self.path) {
            Ok(m) => m.len(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
            Err(err) => {
                tracing::warn!(path = %self.path.display(), error = %err, "cannot read workspace");
                return None;
            }
        };
        if size > MAX_FILE_BYTES {
            tracing::warn!(path = %self.path.display(), size, "workspace file too large; ignored");
            return None;
        }
        let text = std::fs::read_to_string(&self.path)
            .inspect_err(|err| tracing::warn!(error = %err, "cannot read workspace"))
            .ok()?;
        match decode(&text) {
            Ok(workspace) => Some(workspace),
            Err(err) => {
                tracing::warn!(path = %self.path.display(), error = %err, "workspace not restored");
                None
            }
        }
    }

    fn save(&self, workspace: &Workspace) -> Result<(), String> {
        let text = encode(workspace)?;
        match self.writer() {
            Some(writer) => match writer.send(Job::Write(text)) {
                Ok(()) => Ok(()),
                // The writer thread is gone: write here instead.
                Err(mpsc::SendError(Job::Write(text))) => {
                    write_atomic(&self.path, &text).map_err(|e| e.to_string())
                }
                Err(_) => Ok(()),
            },
            None => write_atomic(&self.path, &text).map_err(|e| e.to_string()),
        }
    }

    fn flush(&self) {
        let writer = self
            .writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(writer) = writer {
            let (done, wait) = mpsc::sync_channel(1);
            if writer.send(Job::Flush(done)).is_ok() {
                let _ = wait.recv();
            }
        }
    }
}

/// The writer thread: writes the latest queued workspace, answers flushes
/// once what was queued before them is on disk, and ends when the store is
/// dropped.
fn write_queue(path: &Path, queue: &mpsc::Receiver<Job>) {
    while let Ok(job) = queue.recv() {
        let mut latest = None;
        let mut waiting = Vec::new();
        for job in std::iter::once(job).chain(queue.try_iter()) {
            match job {
                Job::Write(text) => latest = Some(text),
                Job::Flush(done) => waiting.push(done),
            }
        }
        if let Some(text) = latest {
            let started = std::time::Instant::now();
            match write_atomic(path, &text) {
                Ok(()) => tracing::debug!(
                    elapsed_us = started.elapsed().as_micros() as u64,
                    bytes = text.len(),
                    "workspace written"
                ),
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "workspace not saved")
                }
            }
        }
        for done in waiting {
            let _ = done.send(());
        }
    }
}

// The file format. The `*V1` types read and write versions 1 and 2:
// version 2 only added the optional panel `identity`.

#[derive(Debug, Serialize, Deserialize)]
struct FileV1 {
    version: u32,
    active_branch: u64,
    focus_mode: bool,
    retain_focus_on_branch_switch: bool,
    roots: Vec<u64>,
    #[serde(default)]
    back: Vec<u64>,
    #[serde(default)]
    forward: Vec<u64>,
    panels: Vec<PanelV1>,
    branches: Vec<BranchV1>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PanelV1 {
    id: u64,
    location: LocationV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<u64>,
    #[serde(default)]
    children: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<LocationV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scroll: Option<ScrollV1>,
    #[serde(default)]
    show_hidden: bool,
    #[serde(default)]
    sort_key: String,
    #[serde(default)]
    sort_descending: bool,
    #[serde(default)]
    view_mode: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    selection: Vec<LocationV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<LocationV1>,
    /// Version 2: the folder's identity as "device:inode" (a string, because
    /// TOML integers cannot hold every 64-bit inode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScrollV1 {
    item: LocationV1,
    offset: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct BranchV1 {
    id: u64,
    tip: u64,
    cursor: u64,
    created: u64,
    last_active: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocationV1 {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    trash: bool,
    path: PathV1,
}

/// A path as text when it is valid UTF-8, otherwise as raw bytes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum PathV1 {
    Text(String),
    Bytes(Vec<u8>),
}

fn path_to_file(path: &Path) -> PathV1 {
    match path.to_str() {
        Some(text) => PathV1::Text(text.to_owned()),
        None => PathV1::Bytes(path.as_os_str().as_bytes().to_vec()),
    }
}

fn path_from_file(path: PathV1) -> PathBuf {
    match path {
        PathV1::Text(text) => PathBuf::from(text),
        PathV1::Bytes(bytes) => PathBuf::from(OsString::from_vec(bytes)),
    }
}

fn location_to_file(location: &Location) -> LocationV1 {
    match location {
        Location::Path(p) => LocationV1 {
            trash: false,
            path: path_to_file(p),
        },
        Location::Trash(p) => LocationV1 {
            trash: true,
            path: path_to_file(p),
        },
    }
}

fn location_from_file(location: LocationV1) -> Result<Location, String> {
    let path = path_from_file(location.path);
    if location.trash {
        if path.is_absolute() {
            return Err("a Trash location must be relative".into());
        }
        Ok(Location::Trash(path))
    } else if path.is_absolute() {
        Ok(Location::Path(path))
    } else {
        Err(format!("“{}” is not an absolute path", path.display()))
    }
}

fn encode(workspace: &Workspace) -> Result<String, String> {
    let parts = workspace.to_parts();
    let file = FileV1 {
        version: WORKSPACE_SCHEMA_VERSION,
        active_branch: parts.active.raw(),
        focus_mode: parts.focus_mode,
        retain_focus_on_branch_switch: parts.retain_focus_on_branch_switch,
        roots: parts.roots.iter().map(|p| p.raw()).collect(),
        back: parts.back.iter().map(|p| p.raw()).collect(),
        forward: parts.forward.iter().map(|p| p.raw()).collect(),
        panels: parts
            .panels
            .iter()
            .map(|p| {
                let s = &p.state;
                PanelV1 {
                    id: p.id.raw(),
                    location: location_to_file(&p.location),
                    parent: p.parent.map(PanelId::raw),
                    children: p.children.iter().map(|c| c.raw()).collect(),
                    cursor: s.cursor.as_ref().map(location_to_file),
                    scroll: s.scroll.as_ref().map(|a| ScrollV1 {
                        item: location_to_file(&a.item),
                        offset: a.offset,
                    }),
                    show_hidden: s.options.show_hidden,
                    sort_key: s.options.sort.key.id().to_owned(),
                    sort_descending: s.options.sort.descending,
                    view_mode: s.mode.id().to_owned(),
                    query: s.options.query.clone(),
                    selection: s.selection.items().iter().map(location_to_file).collect(),
                    anchor: s.selection.anchor().map(location_to_file),
                    identity: p.identity.map(|i| format!("{}:{}", i.device, i.inode)),
                }
            })
            .collect(),
        branches: parts
            .branches
            .iter()
            .map(|b| BranchV1 {
                id: b.id.raw(),
                tip: b.tip.raw(),
                cursor: b.cursor.raw(),
                created: b.created,
                last_active: b.last_active,
            })
            .collect(),
    };
    toml::to_string(&file).map_err(|e| e.to_string())
}

fn decode(text: &str) -> Result<Workspace, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let version = table
        .get("version")
        .and_then(toml::Value::as_integer)
        .ok_or("the workspace file has no version")?;
    let version = u32::try_from(version).map_err(|_| "invalid version".to_owned())?;
    if version > WORKSPACE_SCHEMA_VERSION {
        return Err(format!(
            "it was saved by a newer Seshat (format {version}, this version reads up to {WORKSPACE_SCHEMA_VERSION})"
        ));
    }
    let table = migrate(table, version)?;
    let file: FileV1 = table
        .try_into()
        .map_err(|e: toml::de::Error| e.to_string())?;

    let mut panels = Vec::with_capacity(file.panels.len());
    for p in file.panels {
        let mut selection = Selection::default();
        selection.replace(
            p.selection
                .into_iter()
                .map(location_from_file)
                .collect::<Result<Vec<_>, _>>()?,
        );
        selection.set_anchor(p.anchor.map(location_from_file).transpose()?);
        let defaults = ViewOptions::default();
        let state = PanelState {
            cursor: p.cursor.map(location_from_file).transpose()?,
            scroll: p
                .scroll
                .map(|s| -> Result<ScrollAnchor, String> {
                    Ok(ScrollAnchor {
                        item: location_from_file(s.item)?,
                        offset: s.offset,
                    })
                })
                .transpose()?,
            options: ViewOptions {
                show_hidden: p.show_hidden,
                sort: SortOrder {
                    key: SortKey::from_id(&p.sort_key).unwrap_or(defaults.sort.key),
                    descending: p.sort_descending,
                },
                query: p.query,
            },
            mode: ViewMode::from_id(&p.view_mode).unwrap_or_default(),
            selection,
        };
        panels.push(PanelParts {
            id: PanelId::from_raw(p.id),
            location: location_from_file(p.location)?,
            parent: p.parent.map(PanelId::from_raw),
            children: p.children.into_iter().map(PanelId::from_raw).collect(),
            identity: p.identity.as_deref().and_then(identity_from_file),
            state,
        });
    }
    let parts = WorkspaceParts {
        panels,
        roots: file.roots.into_iter().map(PanelId::from_raw).collect(),
        branches: file
            .branches
            .into_iter()
            .map(|b| BranchParts {
                id: BranchId::from_raw(b.id),
                tip: PanelId::from_raw(b.tip),
                cursor: PanelId::from_raw(b.cursor),
                created: b.created,
                last_active: b.last_active,
            })
            .collect(),
        active: BranchId::from_raw(file.active_branch),
        focus_mode: file.focus_mode,
        retain_focus_on_branch_switch: file.retain_focus_on_branch_switch,
        back: file.back.into_iter().map(PanelId::from_raw).collect(),
        forward: file.forward.into_iter().map(PanelId::from_raw).collect(),
    };
    Workspace::from_parts(parts).map_err(|e| e.to_string())
}

/// Brings an older file up to the current format, one step per version
/// (e.g. `2 => rename fields`, then fall through to the next).
fn migrate(table: toml::Table, from: u32) -> Result<toml::Table, String> {
    match from {
        // Version 2 only added the optional panel `identity`: a version 1
        // file reads as is, with identities unknown.
        1 | WORKSPACE_SCHEMA_VERSION => Ok(table),
        other => Err(format!("unsupported workspace format {other}")),
    }
}

/// "device:inode" back to an identity; anything else counts as unknown.
fn identity_from_file(text: &str) -> Option<DirectoryIdentity> {
    let (device, inode) = text.split_once(':')?;
    Some(DirectoryIdentity {
        device: device.parse().ok()?,
        inode: inode.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_domain::Direction;

    fn loc(s: &str) -> Location {
        Location::Path(s.into())
    }

    fn sample() -> Workspace {
        let mut ws = Workspace::new(loc("/home/u"), PanelState::default());
        let home = ws.active_panel();
        let docs = ws
            .open_child(home, loc("/home/u/Documents"), None)
            .unwrap()
            .panel;
        let state = ws.state_mut(docs).unwrap();
        state.cursor = Some(loc("/home/u/Documents/a.txt"));
        state.scroll = Some(ScrollAnchor {
            item: loc("/home/u/Documents/a.txt"),
            offset: 40,
        });
        state.options.sort.key = SortKey::Modified;
        state.options.query = "rep".into();
        state.mode = ViewMode::Grid;
        state.selection.replace([loc("/home/u/Documents/b.txt")]);
        ws.move_left();
        ws.open_child(home, loc("/home/u/Downloads"), None).unwrap();
        // A folder whose name is not valid UTF-8.
        let odd = Location::Path(PathBuf::from(OsString::from_vec(
            b"/home/u/caf\xe9".to_vec(),
        )));
        ws.open_root(odd, PanelState::default());
        ws.open_root(Location::Trash("old".into()), PanelState::default());
        ws.set_focus_mode(true);
        ws
    }

    fn store() -> (tempfile::TempDir, TomlWorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlWorkspaceStore::new(dir.path().join("state/workspace.toml"));
        (dir, store)
    }

    #[test]
    fn spec_16_7_round_trips_branches_panel_state_and_focus_mode() {
        let (_dir, store) = store();
        let ws = sample();
        store.save(&ws).unwrap();
        let restored = store.load().expect("restored");
        assert_eq!(restored.to_parts(), ws.to_parts());
        assert!(restored.focus_mode());
        assert_eq!(restored.branch_count(), 4);
    }

    #[test]
    fn background_saves_keep_the_latest_and_flush_waits_for_it() {
        let (_dir, store) = store();
        let mut ws = sample();
        for _ in 0..20 {
            ws.cycle_branch(Direction::Next);
            store.save(&ws).unwrap();
        }
        store.flush();
        let on_disk = std::fs::read_to_string(store.path()).unwrap();
        assert_eq!(decode(&on_disk).unwrap().to_parts(), ws.to_parts());
        // load() flushes too, so it always sees the latest save.
        ws.set_focus_mode(false);
        store.save(&ws).unwrap();
        assert_eq!(store.load().unwrap().to_parts(), ws.to_parts());
    }

    #[test]
    fn missing_file_restores_nothing() {
        let (_dir, store) = store();
        assert!(store.load().is_none());
    }

    #[test]
    fn spec_16_7_newer_and_corrupt_files_are_ignored() {
        let (_dir, store) = store();
        store.save(&sample()).unwrap();
        store.flush(); // the file is read directly below
        let text = std::fs::read_to_string(store.path()).unwrap();
        std::fs::write(store.path(), text.replace("version = 2", "version = 99")).unwrap();
        assert!(store.load().is_none());

        std::fs::write(store.path(), "not = = toml").unwrap();
        assert!(store.load().is_none());

        std::fs::write(store.path(), "version = 1\nroots = []\n").unwrap();
        assert!(store.load().is_none());
    }

    #[test]
    fn structurally_invalid_workspaces_are_rejected() {
        let (_dir, store) = store();
        store.save(&sample()).unwrap();
        store.flush(); // the file is read directly below
        let text = std::fs::read_to_string(store.path()).unwrap();
        // Point the active branch somewhere that does not exist.
        let broken = text
            .lines()
            .map(|l| {
                if l.starts_with("active_branch") {
                    "active_branch = 9999"
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(store.path(), broken).unwrap();
        assert!(store.load().is_none());
    }

    #[test]
    fn relative_paths_are_rejected() {
        let file = r#"
version = 1
active_branch = 2
focus_mode = false
retain_focus_on_branch_switch = true
roots = [1]
[[panels]]
id = 1
location = { path = "relative/dir" }
[[branches]]
id = 2
tip = 1
cursor = 1
created = 1
last_active = 1
"#;
        assert!(decode(file).is_err());
        assert!(decode(&file.replace("relative/dir", "/abs")).is_ok());
    }

    #[test]
    fn version_1_files_restore_with_identities_unknown() {
        let file = r#"
version = 1
active_branch = 3
focus_mode = false
retain_focus_on_branch_switch = true
roots = [1]
[[panels]]
id = 1
location = { path = "/home/u" }
children = [2]
[[panels]]
id = 2
parent = 1
location = { path = "/home/u/Documents" }
cursor = { path = "/home/u/Documents/a.txt" }
[[branches]]
id = 3
tip = 2
cursor = 2
created = 1
last_active = 1
"#;
        let ws = decode(file).expect("a version 1 file still restores");
        assert_eq!(ws.panel_count(), 2);
        assert_eq!(ws.branch_count(), 1, "no duplicate branches");
        assert!(ws.panels().all(|p| p.identity().is_none()));
        let docs = ws.panels().find(|p| p.parent().is_some()).unwrap();
        assert_eq!(docs.state.cursor, Some(loc("/home/u/Documents/a.txt")));
    }

    #[test]
    fn identities_round_trip_including_full_64_bit_inodes() {
        let (_dir, store) = store();
        let mut ws = Workspace::new(loc("/home/u"), PanelState::default());
        let home = ws.active_panel();
        let identity = DirectoryIdentity {
            device: u64::MAX,
            inode: u64::MAX - 1,
        };
        let docs = ws
            .open_directory(home, loc("/home/u/Docs"), Some(identity), None)
            .unwrap()
            .panel;
        store.save(&ws).unwrap();
        store.flush();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(text.contains("version = 2"));
        let restored = store.load().unwrap();
        assert_eq!(restored.panel(docs).unwrap().identity(), Some(identity));
        assert_eq!(
            identity_from_file("12:not-a-number"),
            None,
            "malformed is unknown"
        );
    }
}
