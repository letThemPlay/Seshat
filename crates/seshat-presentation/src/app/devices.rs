//! Storage devices: the mounted and mountable devices the sidebar shows,
//! mounting and ejecting them, and keeping panels on a removed device
//! (decision D14).

use std::path::PathBuf;
use std::rc::Rc;

use relm4::ComponentSender;
use seshat_application::ports::{DeviceError, DeviceMonitor, vanished_roots};
use seshat_application::workspace::WorkspaceService;
use seshat_domain::entities::{Device, DeviceId, Location};

use super::{App, Cx, Msg};
use crate::notice::{self, Notice};
use crate::sidebar::SidebarInput;

#[derive(Debug)]
pub enum DeviceMsg {
    /// Devices were added, removed or changed state.
    Changed,
    Mount(DeviceId),
    /// Eject a device. The window releases its watches first (see
    /// [`DeviceController::root_of`]).
    Eject(DeviceId),
    /// A mount (with its root) or an eject (`None`) finished.
    ActionFinished(Result<Option<PathBuf>, DeviceError>),
}

pub(super) struct DeviceController {
    monitor: Rc<dyn DeviceMonitor>,
    devices: Vec<Device>,
}

impl DeviceController {
    pub(super) fn new(monitor: Rc<dyn DeviceMonitor>) -> Self {
        let devices = monitor.devices();
        Self { monitor, devices }
    }

    /// Reports device changes as [`DeviceMsg::Changed`].
    pub(super) fn connect(&self, sender: &ComponentSender<App>) {
        let sender = sender.clone();
        self.monitor.connect_changed(Box::new(move || {
            sender.input_sender().emit(Msg::Device(DeviceMsg::Changed))
        }));
    }

    pub(super) fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Where a device is mounted. Panels there must stop watching before
    /// the device is ejected, so it isn't kept busy.
    pub(super) fn root_of(&self, id: &DeviceId) -> Option<PathBuf> {
        self.devices
            .iter()
            .find(|d| d.id == *id)
            .and_then(|d| d.root().map(PathBuf::from))
    }

    /// Handles `msg`. Returns what the sidebar must be told, if anything.
    pub(super) fn update(&mut self, msg: DeviceMsg, cx: &mut Cx<'_>) -> Option<SidebarInput> {
        match msg {
            DeviceMsg::Changed => self.changed(cx.service, cx.notice),
            DeviceMsg::Mount(id) => {
                let monitor = self.monitor.clone();
                let sender = cx.sender.clone();
                relm4::spawn_local(async move {
                    let result = monitor.mount(&id).await.map(Some);
                    sender
                        .input_sender()
                        .emit(Msg::Device(DeviceMsg::ActionFinished(result)));
                });
                None
            }
            DeviceMsg::Eject(id) => {
                let monitor = self.monitor.clone();
                let sender = cx.sender.clone();
                relm4::spawn_local(async move {
                    let result = monitor.eject(&id).await.map(|()| None);
                    sender
                        .input_sender()
                        .emit(Msg::Device(DeviceMsg::ActionFinished(result)));
                });
                None
            }
            DeviceMsg::ActionFinished(result) => match result {
                Ok(Some(root)) => {
                    cx.service.open_location(Location::Path(root));
                    None
                }
                Ok(None) => {
                    *cx.notice = Some(notice::device_safe_to_remove());
                    None
                }
                Err(err) => {
                    tracing::warn!(error = %err, "device action failed");
                    *cx.notice = Some(notice::device_action_failed(&err));
                    // Reselect the current location instead of the device row.
                    Some(SidebarInput::SetCurrent(Some(
                        cx.service.active_location().clone(),
                    )))
                }
            },
        }
    }

    /// Re-reads the devices. Panels on a device that went away are kept,
    /// marked unavailable, until the user closes them (D14).
    fn changed(
        &mut self,
        service: &mut WorkspaceService,
        notice: &mut Option<Notice>,
    ) -> Option<SidebarInput> {
        let devices = self.monitor.devices();
        let vanished = vanished_roots(&self.devices, &devices);
        tracing::debug!(
            count = devices.len(),
            vanished = vanished.len(),
            "devices changed"
        );
        self.devices = devices.clone();
        let lost: usize = vanished
            .iter()
            .map(|root| service.detach_device(root))
            .sum();
        if lost > 0 {
            *notice = Some(notice::device_removed());
        }
        Some(SidebarInput::SetDevices(devices))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use seshat_application::LocalBoxFuture;
    use seshat_domain::entities::{DeviceKind, DeviceState};
    use seshat_domain::workspace::{PanelState, Unavailability};

    use super::*;

    #[derive(Default)]
    struct FakeMonitor {
        devices: RefCell<Vec<Device>>,
    }

    impl DeviceMonitor for FakeMonitor {
        fn devices(&self) -> Vec<Device> {
            self.devices.borrow().clone()
        }

        fn connect_changed(&self, _: Box<dyn Fn()>) {}

        fn mount<'a>(
            &'a self,
            _: &'a DeviceId,
        ) -> LocalBoxFuture<'a, Result<PathBuf, DeviceError>> {
            Box::pin(async { Err(DeviceError::NotFound) })
        }

        fn eject<'a>(&'a self, _: &'a DeviceId) -> LocalBoxFuture<'a, Result<(), DeviceError>> {
            Box::pin(async { Ok(()) })
        }
    }

    fn usb() -> Device {
        Device {
            id: DeviceId::new("usb"),
            name: "USB".into(),
            kind: DeviceKind::Removable,
            state: DeviceState::Mounted {
                root: "/run/media/u/USB".into(),
            },
            can_unmount: true,
            can_eject: true,
        }
    }

    #[test]
    fn a_removed_device_keeps_its_panels_and_says_so() {
        let monitor = Rc::new(FakeMonitor::default());
        monitor.devices.borrow_mut().push(usb());
        let mut devices = DeviceController::new(monitor.clone());
        assert_eq!(
            devices.root_of(&DeviceId::new("usb")),
            Some(PathBuf::from("/run/media/u/USB"))
        );

        let mut service =
            WorkspaceService::starting_at(Location::Path("/home/u".into()), PanelState::default());
        service.open_location(Location::Path("/run/media/u/USB/photos".into()));
        let on_usb = service.active_panel();
        let mut notice = None;

        monitor.devices.borrow_mut().clear();
        let sidebar = devices.changed(&mut service, &mut notice);

        assert!(matches!(sidebar, Some(SidebarInput::SetDevices(d)) if d.is_empty()));
        assert!(devices.devices().is_empty());
        assert_eq!(
            service.workspace().panel(on_usb).unwrap().unavailable(),
            Some(Unavailability::DeviceRemoved),
            "the panel is kept, marked unavailable"
        );
        assert_eq!(
            notice.as_ref().map(Notice::text),
            Some("The device was removed. Its panels are kept until you close them.")
        );
    }

    #[test]
    fn a_new_device_only_updates_the_sidebar() {
        let monitor = Rc::new(FakeMonitor::default());
        let mut devices = DeviceController::new(monitor.clone());
        let mut service =
            WorkspaceService::starting_at(Location::Path("/home/u".into()), PanelState::default());
        let mut notice = None;
        monitor.devices.borrow_mut().push(usb());
        let sidebar = devices.changed(&mut service, &mut notice);
        assert!(matches!(sidebar, Some(SidebarInput::SetDevices(d)) if d.len() == 1));
        assert_eq!(notice, None);
        assert_eq!(devices.devices().len(), 1);
    }
}
