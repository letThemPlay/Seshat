use std::path::PathBuf;

use seshat_domain::entities::{Device, DeviceId, Location};
use thiserror::Error;

use crate::LocalBoxFuture;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeviceError {
    #[error("device is no longer available")]
    NotFound,
    #[error("device is busy")]
    Busy,
    #[error("{0}")]
    Failed(String),
}

/// Observes mounted and mountable storage devices.
pub trait DeviceMonitor {
    fn devices(&self) -> Vec<Device>;

    /// Registers a callback invoked whenever devices are added, removed or
    /// change state. Callbacks live as long as the monitor.
    fn connect_changed(&self, on_change: Box<dyn Fn()>);

    /// Mounts a device and returns its root.
    fn mount<'a>(&'a self, id: &'a DeviceId) -> LocalBoxFuture<'a, Result<PathBuf, DeviceError>>;

    /// Unmounts (and ejects, where the device supports it) a device.
    fn eject<'a>(&'a self, id: &'a DeviceId) -> LocalBoxFuture<'a, Result<(), DeviceError>>;
}

/// Whether `current` lived on a device that disappeared between `before` and
/// `after` (unplugged, unmounted or ejected).
pub fn location_lost(current: &Location, before: &[Device], after: &[Device]) -> bool {
    before
        .iter()
        .filter_map(Device::root)
        .filter(|root| !after.iter().any(|d| d.root() == Some(root)))
        .any(|root| current.is_within(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_domain::entities::{DeviceKind, DeviceState};

    fn mounted(id: &str, root: &str) -> Device {
        Device {
            id: DeviceId::new(id),
            name: id.to_owned(),
            kind: DeviceKind::Removable,
            state: DeviceState::Mounted { root: root.into() },
            can_unmount: true,
            can_eject: true,
        }
    }

    #[test]
    fn detects_removed_device_under_current_location() {
        let before = [
            mounted("usb", "/run/media/u/USB"),
            mounted("sd", "/run/media/u/SD"),
        ];
        let after = [mounted("sd", "/run/media/u/SD")];
        let here = Location::Path("/run/media/u/USB/photos".into());
        assert!(location_lost(&here, &before, &after));
    }

    #[test]
    fn unrelated_removal_is_ignored() {
        let before = [mounted("usb", "/run/media/u/USB")];
        let here = Location::Path("/home/u".into());
        assert!(!location_lost(&here, &before, &[]));
        assert!(!location_lost(&Location::trash_root(), &before, &[]));
    }

    #[test]
    fn unmounted_device_counts_as_lost() {
        let before = [mounted("usb", "/run/media/u/USB")];
        let mut after = before.clone();
        after[0].state = DeviceState::Unmounted;
        let here = Location::Path("/run/media/u/USB".into());
        assert!(location_lost(&here, &before, &after));
    }
}
