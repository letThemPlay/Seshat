use std::fmt;
use std::path::PathBuf;

/// Stable identifier of a storage device for the lifetime of the session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    /// USB sticks, SD cards, external disks.
    Removable,
    /// Network shares (SMB, SFTP, NFS, …).
    Network,
    /// Internal, non-system drives.
    Drive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    Mounted {
        root: PathBuf,
    },
    /// Present but not mounted; can be mounted on request.
    Unmounted,
}

/// A mounted volume or mountable device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub kind: DeviceKind,
    pub state: DeviceState,
    pub can_unmount: bool,
    pub can_eject: bool,
}

impl Device {
    pub fn root(&self) -> Option<&std::path::Path> {
        match &self.state {
            DeviceState::Mounted { root } => Some(root),
            DeviceState::Unmounted => None,
        }
    }
}
