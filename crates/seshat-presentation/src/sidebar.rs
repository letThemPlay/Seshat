//! Left navigation pane: standard places and storage devices.

use gtk::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use seshat_domain::entities::{
    Device, DeviceId, DeviceKind, DeviceState, Location, StandardLocation, StandardLocationKind,
};

#[derive(Debug)]
pub struct SidebarInit {
    pub places: Vec<StandardLocation>,
    pub devices: Vec<Device>,
}

#[derive(Debug)]
pub enum SidebarInput {
    SetDevices(Vec<Device>),
    /// Highlights the row matching the current location, if any.
    SetCurrent(Option<Location>),
    RowSelected(usize),
    EjectClicked(DeviceId),
}

#[derive(Debug)]
pub enum SidebarOutput {
    Navigate(Location),
    Mount(DeviceId),
    Eject(DeviceId),
}

/// What a row leads to. Headings have no target.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Location(Location),
    UnmountedVolume(DeviceId),
}

#[derive(Debug)]
pub struct Sidebar {
    list: gtk::ListBox,
    places: Vec<StandardLocation>,
    devices: Vec<Device>,
    /// One entry per row, in row order.
    targets: Vec<Option<Target>>,
    current: Option<Location>,
}

impl SimpleComponent for Sidebar {
    type Input = SidebarInput;
    type Output = SidebarOutput;
    type Init = SidebarInit;
    type Root = gtk::ScrolledWindow;
    type Widgets = ();

    fn init_root() -> gtk::ScrolledWindow {
        gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .css_classes(["seshat-sidebar"])
            .build()
    }

    fn init(
        init: SidebarInit,
        root: gtk::ScrolledWindow,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        list.update_property(&[gtk::accessible::Property::Label("Places")]);
        list.connect_row_selected({
            let sender = sender.clone();
            move |_, row| {
                if let Some(index) = row.and_then(|r| usize::try_from(r.index()).ok()) {
                    sender.input(SidebarInput::RowSelected(index));
                }
            }
        });
        root.set_child(Some(&list));

        let mut model = Sidebar {
            list,
            places: init.places,
            devices: init.devices,
            targets: Vec::new(),
            current: None,
        };
        model.rebuild(&sender);
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: SidebarInput, sender: ComponentSender<Self>) {
        match msg {
            SidebarInput::SetDevices(devices) => {
                if devices != self.devices {
                    self.devices = devices;
                    self.rebuild(&sender);
                }
            }
            SidebarInput::SetCurrent(location) => {
                self.current = location;
                self.sync_selection();
            }
            SidebarInput::RowSelected(index) => {
                let output = match self.targets.get(index).cloned().flatten() {
                    Some(Target::Location(location)) => SidebarOutput::Navigate(location),
                    Some(Target::UnmountedVolume(id)) => SidebarOutput::Mount(id),
                    None => return,
                };
                let _ = sender.output(output);
            }
            SidebarInput::EjectClicked(id) => {
                let _ = sender.output(SidebarOutput::Eject(id));
            }
        }
    }
}

impl Sidebar {
    fn rebuild(&mut self, sender: &ComponentSender<Self>) {
        self.list.remove_all();
        self.targets.clear();

        for place in &self.places {
            self.list
                .append(&row(place_icon(place.kind), &place.label, None, false));
            self.targets
                .push(Some(Target::Location(place.location.clone())));
        }

        if !self.devices.is_empty() {
            self.list.append(&heading("Devices"));
            self.targets.push(None);
        }
        for device in &self.devices {
            let eject = (device.can_eject || device.can_unmount).then(|| {
                let button = gtk::Button::builder()
                    .icon_name("media-eject-symbolic")
                    .tooltip_text(if device.can_eject { "Eject" } else { "Unmount" })
                    .css_classes(["flat", "seshat-eject"])
                    .valign(gtk::Align::Center)
                    .build();
                let sender = sender.clone();
                let id = device.id.clone();
                button
                    .connect_clicked(move |_| sender.input(SidebarInput::EjectClicked(id.clone())));
                button.upcast::<gtk::Widget>()
            });
            let unmounted = device.state == DeviceState::Unmounted;
            self.list.append(&row(
                device_icon(device.kind),
                &device.name,
                eject,
                unmounted,
            ));
            self.targets.push(Some(match &device.state {
                DeviceState::Mounted { root } => Target::Location(Location::Path(root.clone())),
                DeviceState::Unmounted => Target::UnmountedVolume(device.id.clone()),
            }));
        }
        self.sync_selection();
    }

    fn sync_selection(&self) {
        let wanted = self.current.as_ref().and_then(|current| {
            self.targets
                .iter()
                .position(|t| matches!(t, Some(Target::Location(l)) if l == current))
        });
        let row = wanted
            .and_then(|i| i32::try_from(i).ok())
            .and_then(|i| self.list.row_at_index(i));
        match row {
            Some(row) if !row.is_selected() => self.list.select_row(Some(&row)),
            Some(_) => {}
            None => self.list.unselect_all(),
        }
    }
}

fn row(icon: &str, label: &str, suffix: Option<gtk::Widget>, dimmed: bool) -> gtk::ListBoxRow {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.append(&gtk::Image::from_icon_name(icon));
    let text = gtk::Label::builder()
        .label(label)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    if dimmed {
        text.add_css_class("dim-label");
    }
    content.append(&text);
    if let Some(suffix) = suffix {
        content.append(&suffix);
    }
    let row = gtk::ListBoxRow::builder().child(&content).build();
    row.set_tooltip_text(Some(label));
    row
}

fn heading(label: &str) -> gtk::ListBoxRow {
    let text = gtk::Label::builder().label(label).xalign(0.0).build();
    let row = gtk::ListBoxRow::builder()
        .child(&text)
        .selectable(false)
        .activatable(false)
        .css_classes(["seshat-sidebar-heading"])
        .build();
    row.set_focusable(false);
    row
}

fn place_icon(kind: StandardLocationKind) -> &'static str {
    match kind {
        StandardLocationKind::Home => "user-home-symbolic",
        StandardLocationKind::Desktop => "user-desktop-symbolic",
        StandardLocationKind::Documents => "folder-documents-symbolic",
        StandardLocationKind::Downloads => "folder-download-symbolic",
        StandardLocationKind::Pictures => "folder-pictures-symbolic",
        StandardLocationKind::Music => "folder-music-symbolic",
        StandardLocationKind::Videos => "folder-videos-symbolic",
        StandardLocationKind::Trash => "user-trash-symbolic",
    }
}

fn device_icon(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Removable => "drive-removable-media-symbolic",
        DeviceKind::Network => "folder-remote-symbolic",
        DeviceKind::Drive => "drive-harddisk-symbolic",
    }
}
