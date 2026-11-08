use std::path::{Path, PathBuf};
use std::rc::Rc;

use gio::prelude::*;
use seshat_application::LocalBoxFuture;
use seshat_application::ports::{DeviceError, DeviceMonitor};
use seshat_domain::entities::{Device, DeviceId, DeviceKind, DeviceState};

/// Storage devices from GIO's `GVolumeMonitor` (UDisks2, GVfs network mounts).
///
/// System-internal mounts (`/`, `/boot`, …) are already filtered out by GIO's
/// user-visibility rules.
#[derive(Debug)]
pub struct GioDeviceMonitor {
    monitor: gio::VolumeMonitor,
}

impl Default for GioDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl GioDeviceMonitor {
    /// Must be created on the thread running the GLib main loop; change
    /// signals are delivered there.
    pub fn new() -> Self {
        Self {
            monitor: gio::VolumeMonitor::get(),
        }
    }

    fn find_volume(&self, id: &DeviceId) -> Option<gio::Volume> {
        self.monitor
            .volumes()
            .into_iter()
            .find(|v| volume_id(v) == *id)
    }

    fn find_mount(&self, id: &DeviceId) -> Option<gio::Mount> {
        self.monitor
            .mounts()
            .into_iter()
            .find(|m| mount_id(m) == *id)
    }
}

impl DeviceMonitor for GioDeviceMonitor {
    fn devices(&self) -> Vec<Device> {
        let mut devices: Vec<Device> = self
            .monitor
            .mounts()
            .iter()
            .filter(|m| !m.is_shadowed())
            .filter_map(mounted_device)
            .collect();

        devices.extend(
            self.monitor
                .volumes()
                .iter()
                .filter(|v| v.get_mount().is_none() && v.can_mount())
                .map(|v| Device {
                    id: volume_id(v),
                    name: v.name().to_string(),
                    kind: kind(None, v.drive().as_ref(), v.can_eject()),
                    state: DeviceState::Unmounted,
                    can_unmount: false,
                    can_eject: v.can_eject(),
                }),
        );
        devices.sort_by_key(|d| d.name.to_lowercase());
        devices
    }

    fn connect_changed(&self, on_change: Box<dyn Fn()>) {
        let on_change: Rc<dyn Fn()> = Rc::from(on_change);
        macro_rules! forward {
            ($connect:ident) => {{
                let f = on_change.clone();
                self.monitor.$connect(move |_, _| f());
            }};
        }
        forward!(connect_mount_added);
        forward!(connect_mount_removed);
        forward!(connect_mount_changed);
        forward!(connect_volume_added);
        forward!(connect_volume_removed);
        forward!(connect_volume_changed);
    }

    fn mount<'a>(&'a self, id: &'a DeviceId) -> LocalBoxFuture<'a, Result<PathBuf, DeviceError>> {
        Box::pin(async move {
            let volume = self.find_volume(id).ok_or(DeviceError::NotFound)?;
            // No interactive MountOperation yet: encrypted volumes that need
            // a passphrase fail with a clear error instead of hanging.
            volume
                .mount_future(gio::MountMountFlags::NONE, gio::MountOperation::NONE)
                .await
                .map_err(|err| device_error(&err))?;
            volume
                .get_mount()
                .and_then(|m| m.root().path())
                .ok_or_else(|| DeviceError::Failed("mounted volume has no local path".to_owned()))
        })
    }

    fn eject<'a>(&'a self, id: &'a DeviceId) -> LocalBoxFuture<'a, Result<(), DeviceError>> {
        Box::pin(async move {
            let mount = self.find_mount(id).ok_or(DeviceError::NotFound)?;
            let result = if mount.can_eject() {
                mount
                    .eject_with_operation_future(
                        gio::MountUnmountFlags::NONE,
                        gio::MountOperation::NONE,
                    )
                    .await
            } else {
                mount
                    .unmount_with_operation_future(
                        gio::MountUnmountFlags::NONE,
                        gio::MountOperation::NONE,
                    )
                    .await
            };
            result.map_err(|err| device_error(&err))
        })
    }
}

fn mounted_device(mount: &gio::Mount) -> Option<Device> {
    let root = mount.root();
    // Mounts without a local path (no GVfs FUSE) cannot be browsed yet.
    let path = root.path()?;
    let network = root.uri_scheme().is_some_and(|s| s != "file") || is_gvfs_fuse(&path);
    Some(Device {
        id: mount_id(mount),
        name: mount.name().to_string(),
        kind: if network {
            DeviceKind::Network
        } else {
            kind(Some(mount), mount.drive().as_ref(), mount.can_eject())
        },
        state: DeviceState::Mounted { root: path },
        can_unmount: mount.can_unmount(),
        can_eject: mount.can_eject(),
    })
}

fn kind(mount: Option<&gio::Mount>, drive: Option<&gio::Drive>, can_eject: bool) -> DeviceKind {
    let removable = drive.is_some_and(|d| d.is_removable() || d.is_media_removable());
    let networked = mount.is_some_and(|m| m.volume().is_none() && m.drive().is_none());
    if networked && !can_eject {
        DeviceKind::Network
    } else if removable || can_eject {
        DeviceKind::Removable
    } else {
        DeviceKind::Drive
    }
}

/// GVfs exposes network mounts through FUSE under `$XDG_RUNTIME_DIR/gvfs`.
fn is_gvfs_fuse(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "gvfs")
}

/// Mounts backed by a volume share the volume's id, so a device keeps its
/// identity across mount/unmount.
fn mount_id(mount: &gio::Mount) -> DeviceId {
    match mount.volume() {
        Some(volume) => volume_id(&volume),
        None => DeviceId::new(format!("mount:{}", mount.root().uri())),
    }
}

fn volume_id(volume: &gio::Volume) -> DeviceId {
    let id = ["uuid", "unix-device", "label"]
        .iter()
        .find_map(|kind| volume.identifier(kind))
        .map(|s| s.to_string())
        .unwrap_or_else(|| volume.name().to_string());
    DeviceId::new(format!("volume:{id}"))
}

fn device_error(err: &glib::Error) -> DeviceError {
    match err.kind::<gio::IOErrorEnum>() {
        Some(gio::IOErrorEnum::Busy) => DeviceError::Busy,
        _ => DeviceError::Failed(err.message().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gvfs_fuse_paths_are_network() {
        assert!(is_gvfs_fuse(Path::new(
            "/run/user/1000/gvfs/smb-share:server=x,share=y"
        )));
        assert!(!is_gvfs_fuse(Path::new("/run/media/u/USB")));
    }

    #[test]
    fn listing_devices_does_not_panic_and_ids_are_unique() {
        let context = glib::MainContext::new();
        let devices = context
            .with_thread_default(|| GioDeviceMonitor::new().devices())
            .unwrap();
        let mut ids: Vec<_> = devices.iter().map(|d| d.id.clone()).collect();
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ids.dedup();
        assert_eq!(ids.len(), devices.len());
    }
}
