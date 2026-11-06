//! Main window component.

mod shortcuts;
mod view;

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use relm4::typed_view::list::TypedListView;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use seshat_application::navigation::{NavigationHistory, breadcrumbs, parse_address};
use seshat_application::ports::{
    DeviceError, DeviceMonitor, DirectoryChange, DirectoryWatcher, Launcher, ListingError,
    WatchGuard, location_lost,
};
use seshat_application::use_cases::{ListDirectory, ListingOptions};
use seshat_domain::entities::{Device, DeviceId, FileEntry, Location, StandardLocation};

use crate::content::FileRow;
use crate::diagnostics;
use crate::pathbar::address_text;
use crate::sidebar::{Sidebar, SidebarInit, SidebarInput, SidebarOutput};
use crate::theme::{ThemePreference, render_css};
use view::AppWidgets;

/// Seshat owns its look through its own theme system, so its stylesheet must
/// win over a desktop-wide `~/.config/gtk-4.0/gtk.css` (USER priority);
/// otherwise mandated rules such as the shared surface token and 12px
/// selected radius could be overridden. All rules are scoped to `.seshat*`.
const THEME_PROVIDER_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_USER + 1;

/// Bursts of filesystem events (e.g. extracting an archive) are coalesced
/// into one reload.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(300);

/// Dependencies supplied by the composition root.
pub struct AppInit {
    pub home: PathBuf,
    pub list_directory: ListDirectory,
    pub locations: Vec<StandardLocation>,
    pub directory_watcher: Rc<dyn DirectoryWatcher>,
    pub device_monitor: Rc<dyn DeviceMonitor>,
    pub launcher: Rc<dyn Launcher>,
}

impl std::fmt::Debug for AppInit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppInit").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Msg {
    Navigate(Location),
    Back,
    Forward,
    GoUp,
    Refresh,
    Activate(u32),
    ToggleHidden(bool),
    ToggleHiddenShortcut,
    CycleTheme,
    SystemColorSchemeChanged,
    EditAddress,
    CancelAddress,
    AddressSubmitted(String),
    Loaded {
        generation: u64,
        result: ListingResult,
    },
    DirectoryChanged {
        view: u64,
        change: DirectoryChange,
    },
    ReloadNow {
        view: u64,
    },
    DevicesChanged,
    Mount(DeviceId),
    Eject(DeviceId),
    DeviceActionFinished(Result<Option<PathBuf>, DeviceError>),
    OpenFailed(String),
}

/// Listing result whose `Debug` output is a summary. Relm4 logs messages at
/// debug level, and full listings (file names) do not belong in logs.
pub struct ListingResult(Result<Vec<FileEntry>, ListingError>);

impl std::fmt::Debug for ListingResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Ok(entries) => write!(f, "Ok({} entries)", entries.len()),
            Err(err) => write!(f, "Err({err:?})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContentState {
    Loading,
    Ready,
    Empty,
    Error(String),
}

pub struct App {
    home: PathBuf,
    list_directory: ListDirectory,
    directory_watcher: Rc<dyn DirectoryWatcher>,
    device_monitor: Rc<dyn DeviceMonitor>,
    launcher: Rc<dyn Launcher>,
    sidebar: Controller<Sidebar>,

    history: NavigationHistory,
    devices: Vec<Device>,
    show_hidden: bool,
    state: ContentState,
    entries: TypedListView<FileRow, gtk::MultiSelection>,
    /// Transient message for the status bar (errors, device events).
    notice: Option<String>,
    editing_address: bool,

    /// Incremented per shown location; stale watcher events are dropped.
    view_generation: u64,
    /// Incremented per listing; stale listing results are dropped.
    load_generation: u64,
    load_started: Option<Instant>,
    watch: Option<WatchGuard>,
    reload_pending: bool,

    theme_preference: ThemePreference,
    css_provider: gtk::CssProvider,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("current", &self.history.current())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl SimpleComponent for App {
    type Input = Msg;
    type Output = ();
    type Init = AppInit;
    type Root = gtk::Window;
    type Widgets = AppWidgets;

    fn init_root() -> gtk::Window {
        gtk::Window::builder()
            .title("Seshat")
            .default_width(1040)
            .default_height(680)
            .css_classes(["seshat"])
            .build()
    }

    fn init(
        init: AppInit,
        root: gtk::Window,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let backend = diagnostics::detect_backend();
        diagnostics::log_backend(&backend);

        let css_provider = gtk::CssProvider::new();
        css_provider.connect_parsing_error(|_, section, error| {
            tracing::error!(location = %section, error = %error, "theme stylesheet error");
        });
        gtk::style_context_add_provider_for_display(
            &WidgetExt::display(&root),
            &css_provider,
            THEME_PROVIDER_PRIORITY,
        );
        if let Some(settings) = gtk::Settings::default() {
            let sender = sender.clone();
            settings.connect_gtk_interface_color_scheme_notify(move |_| {
                sender.input(Msg::SystemColorSchemeChanged);
            });
        }

        let devices = init.device_monitor.devices();
        init.device_monitor.connect_changed(Box::new({
            let sender = sender.clone();
            move || sender.input(Msg::DevicesChanged)
        }));

        let sidebar = Sidebar::builder()
            .launch(SidebarInit {
                places: init.locations,
                devices: devices.clone(),
            })
            .forward(sender.input_sender(), |output| match output {
                SidebarOutput::Navigate(location) => Msg::Navigate(location),
                SidebarOutput::Mount(id) => Msg::Mount(id),
                SidebarOutput::Eject(id) => Msg::Eject(id),
            });

        let entries: TypedListView<FileRow, gtk::MultiSelection> = TypedListView::new();
        let widgets = view::build(&root, sidebar.widget(), &entries, &sender);
        shortcuts::install(&root, &sender);

        let mut model = App {
            home: init.home,
            list_directory: init.list_directory,
            directory_watcher: init.directory_watcher,
            device_monitor: init.device_monitor,
            launcher: init.launcher,
            sidebar,
            history: NavigationHistory::default(),
            devices,
            show_hidden: false,
            state: ContentState::Loading,
            entries,
            notice: None,
            editing_address: false,
            view_generation: 0,
            load_generation: 0,
            load_started: None,
            watch: None,
            reload_pending: false,
            theme_preference: ThemePreference::System,
            css_provider,
        };
        model.apply_theme();
        model.go(Location::Path(model.home.clone()), &sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        match msg {
            Msg::Navigate(location) => self.go(location, &sender),
            Msg::Back => {
                if self.history.go_back().is_some() {
                    self.show_current(&sender);
                }
            }
            Msg::Forward => {
                if self.history.go_forward().is_some() {
                    self.show_current(&sender);
                }
            }
            Msg::GoUp => {
                if let Some(parent) = self.history.current().and_then(Location::parent) {
                    self.go(parent, &sender);
                }
            }
            Msg::Refresh => self.show_current(&sender),
            Msg::Activate(position) => self.activate(position, &sender),
            Msg::ToggleHidden(show) => self.set_show_hidden(show, &sender),
            Msg::ToggleHiddenShortcut => self.set_show_hidden(!self.show_hidden, &sender),
            Msg::CycleTheme => {
                self.theme_preference = self.theme_preference.next();
                self.apply_theme();
            }
            Msg::SystemColorSchemeChanged => {
                if self.theme_preference == ThemePreference::System {
                    self.apply_theme();
                }
            }
            Msg::EditAddress => self.editing_address = true,
            Msg::CancelAddress => self.editing_address = false,
            Msg::AddressSubmitted(text) => {
                match parse_address(&text, &self.home, self.history.current()) {
                    Ok(location) => {
                        self.editing_address = false;
                        self.go(location, &sender);
                    }
                    Err(err) => self.notice = Some(err.to_string()),
                }
            }
            Msg::Loaded { generation, result } => self.finish_load(generation, result.0),
            Msg::DirectoryChanged { view, change } => {
                self.directory_changed(view, change, &sender);
            }
            Msg::ReloadNow { view } => {
                self.reload_pending = false;
                if view == self.view_generation
                    && let Some(location) = self.history.current().cloned()
                {
                    self.start_listing(location, &sender);
                }
            }
            Msg::DevicesChanged => self.devices_changed(&sender),
            Msg::Mount(id) => {
                let monitor = self.device_monitor.clone();
                let sender = sender.clone();
                relm4::spawn_local(async move {
                    let result = monitor.mount(&id).await.map(Some);
                    sender.input(Msg::DeviceActionFinished(result));
                });
            }
            Msg::Eject(id) => self.eject(id, &sender),
            Msg::DeviceActionFinished(result) => match result {
                Ok(Some(root)) => self.go(Location::Path(root), &sender),
                Ok(None) => self.notice = Some("The device can be removed safely.".to_owned()),
                Err(err) => {
                    tracing::warn!(error = %err, "device action failed");
                    self.notice = Some(format!("Device action failed: {err}"));
                    // Reselect the current location instead of the device row.
                    self.sidebar
                        .emit(SidebarInput::SetCurrent(self.history.current().cloned()));
                }
            },
            Msg::OpenFailed(err) => self.notice = Some(format!("Could not open the file: {err}")),
        }
    }

    fn update_view(&self, widgets: &mut AppWidgets, sender: ComponentSender<Self>) {
        let current = self.history.current();

        widgets
            .back_button
            .set_sensitive(self.history.can_go_back());
        widgets
            .forward_button
            .set_sensitive(self.history.can_go_forward());
        widgets
            .up_button
            .set_sensitive(current.and_then(Location::parent).is_some());

        if widgets.rendered_crumbs.as_ref() != current {
            if let Some(location) = current {
                let sender = sender.clone();
                widgets
                    .pathbar
                    .set_crumbs(&breadcrumbs(location, &self.home), move |target| {
                        sender.input(Msg::Navigate(target));
                    });
            }
            widgets.rendered_crumbs = current.cloned();
        }
        if self.editing_address != widgets.editing_address {
            widgets.editing_address = self.editing_address;
            if self.editing_address {
                widgets
                    .pathbar
                    .show_entry(&current.map(address_text).unwrap_or_default());
            } else {
                widgets.pathbar.show_crumbs();
            }
        }

        widgets
            .theme_button
            .set_label(&format!("Theme: {}", self.theme_preference.label()));
        if widgets.hidden_toggle.is_active() != self.show_hidden {
            widgets.hidden_toggle.set_active(self.show_hidden);
        }

        widgets.status_title.remove_css_class("seshat-error");
        let (page, message, status) = match &self.state {
            ContentState::Loading => ("status", "Loading…".to_owned(), "Loading…".to_owned()),
            ContentState::Empty => (
                "status",
                if current.is_some_and(Location::is_trash) {
                    "Trash is empty".to_owned()
                } else {
                    "This folder is empty".to_owned()
                },
                "0 items".to_owned(),
            ),
            ContentState::Error(err) => {
                widgets.status_title.add_css_class("seshat-error");
                ("status", err.clone(), "Error".to_owned())
            }
            ContentState::Ready => {
                let count = self.entries.len();
                let plural = if count == 1 { "" } else { "s" };
                ("list", String::new(), format!("{count} item{plural}"))
            }
        };
        widgets.stack.set_visible_child_name(page);
        widgets.status_title.set_label(&message);

        match &self.notice {
            Some(notice) => {
                widgets.status_bar.set_label(notice);
                widgets.status_bar.add_css_class("seshat-notice");
            }
            None => {
                widgets.status_bar.set_label(&status);
                widgets.status_bar.remove_css_class("seshat-notice");
            }
        }
    }
}

impl App {
    /// Navigates to `location`, recording history. Going to the current
    /// location is a no-op (use Refresh to reload).
    fn go(&mut self, location: Location, sender: &ComponentSender<Self>) {
        self.editing_address = false;
        if self.history.visit(location) {
            self.show_current(sender);
        }
    }

    /// Shows the history's current location from scratch: new watch, new
    /// listing.
    fn show_current(&mut self, sender: &ComponentSender<Self>) {
        let Some(location) = self.history.current().cloned() else {
            return;
        };
        tracing::debug!(?location, "showing location");
        self.view_generation += 1;
        self.reload_pending = false;
        self.notice = None;
        self.state = ContentState::Loading;
        self.entries.clear();

        // Watch before listing so changes between the two aren't missed.
        self.watch = None;
        let view = self.view_generation;
        let watch_sender = sender.clone();
        match self.directory_watcher.watch(
            &location,
            Box::new(move |change| watch_sender.input(Msg::DirectoryChanged { view, change })),
        ) {
            Ok(guard) => self.watch = Some(guard),
            Err(err) => tracing::debug!(error = %err, "cannot watch location; no live updates"),
        }

        self.sidebar
            .emit(SidebarInput::SetCurrent(Some(location.clone())));
        self.start_listing(location, sender);
    }

    /// Starts an asynchronous listing. The UI keeps running; the result
    /// arrives as [`Msg::Loaded`]. The visible list is kept until then.
    fn start_listing(&mut self, location: Location, sender: &ComponentSender<Self>) {
        self.load_generation += 1;
        self.load_started = Some(Instant::now());
        let generation = self.load_generation;
        let use_case = self.list_directory.clone();
        let options = ListingOptions {
            show_hidden: self.show_hidden,
        };
        let sender = sender.clone();
        relm4::spawn_local(async move {
            let result = use_case.execute(&location, options).await;
            sender.input(Msg::Loaded {
                generation,
                result: ListingResult(result),
            });
        });
    }

    fn finish_load(&mut self, generation: u64, result: Result<Vec<FileEntry>, ListingError>) {
        if generation != self.load_generation {
            return; // superseded by a newer listing
        }
        let elapsed = self.load_started.take().map(|t| t.elapsed());
        self.entries.clear();
        match result {
            Ok(entries) => {
                tracing::debug!(count = entries.len(), ?elapsed, "listing loaded");
                self.state = if entries.is_empty() {
                    ContentState::Empty
                } else {
                    ContentState::Ready
                };
                self.entries
                    .extend_from_iter(entries.into_iter().map(FileRow));
            }
            Err(err) => {
                tracing::warn!(error = %err, "listing failed");
                self.state = ContentState::Error(error_message(&err));
            }
        }
    }

    fn directory_changed(
        &mut self,
        view: u64,
        change: DirectoryChange,
        sender: &ComponentSender<Self>,
    ) {
        if view != self.view_generation {
            return;
        }
        match change {
            DirectoryChange::Removed => {
                self.fall_back("The folder was deleted or moved.", sender);
            }
            DirectoryChange::Modified if !self.reload_pending => {
                self.reload_pending = true;
                let sender = sender.clone();
                gtk::glib::timeout_add_local_once(RELOAD_DEBOUNCE, move || {
                    sender.input(Msg::ReloadNow { view });
                });
            }
            DirectoryChange::Modified => {}
        }
    }

    fn devices_changed(&mut self, sender: &ComponentSender<Self>) {
        let devices = self.device_monitor.devices();
        let lost = self
            .history
            .current()
            .is_some_and(|current| location_lost(current, &self.devices, &devices));
        tracing::debug!(count = devices.len(), lost, "devices changed");
        self.devices = devices.clone();
        self.sidebar.emit(SidebarInput::SetDevices(devices));
        if lost {
            self.fall_back("The device was removed.", sender);
        }
    }

    fn eject(&mut self, id: DeviceId, sender: &ComponentSender<Self>) {
        // Leave the device first so we don't keep it busy.
        let root = self
            .devices
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.root().map(PathBuf::from));
        if let Some(root) = root
            && self.history.current().is_some_and(|c| c.is_within(&root))
        {
            self.go(Location::Path(self.home.clone()), sender);
        }
        let monitor = self.device_monitor.clone();
        let sender = sender.clone();
        relm4::spawn_local(async move {
            let result = monitor.eject(&id).await.map(|()| None);
            sender.input(Msg::DeviceActionFinished(result));
        });
    }

    /// The current location became unavailable: show Home instead, without
    /// adding a history entry, and tell the user why.
    fn fall_back(&mut self, reason: &str, sender: &ComponentSender<Self>) {
        tracing::info!(reason, "current location unavailable; returning home");
        self.history
            .replace_current(Location::Path(self.home.clone()));
        self.show_current(sender);
        self.notice = Some(reason.to_owned());
    }

    fn activate(&mut self, position: u32, sender: &ComponentSender<Self>) {
        let Some((location, browsable)) = self.entries.get(position).map(|item| {
            let row = item.borrow();
            (row.0.location.clone(), row.0.is_browsable())
        }) else {
            return;
        };
        if browsable {
            self.go(location, sender);
        } else {
            let launcher = self.launcher.clone();
            let sender = sender.clone();
            relm4::spawn_local(async move {
                if let Err(err) = launcher.open(&location).await {
                    sender.input(Msg::OpenFailed(err));
                }
            });
        }
    }

    fn set_show_hidden(&mut self, show: bool, sender: &ComponentSender<Self>) {
        if show == self.show_hidden {
            return;
        }
        self.show_hidden = show;
        if let Some(location) = self.history.current().cloned() {
            self.start_listing(location, sender);
        }
    }

    fn apply_theme(&self) {
        let theme = self.theme_preference.resolve(system_prefers_dark());
        tracing::debug!(theme = %theme.name, preference = ?self.theme_preference, "applying theme");
        self.css_provider.load_from_string(&render_css(&theme));
    }
}

fn system_prefers_dark() -> bool {
    gtk::Settings::default()
        .is_some_and(|s| s.gtk_interface_color_scheme() == gtk::InterfaceColorScheme::Dark)
}

fn error_message(err: &ListingError) -> String {
    match err {
        ListingError::NotFound => "This location no longer exists.".to_owned(),
        ListingError::PermissionDenied => {
            "You don't have permission to view this folder.".to_owned()
        }
        ListingError::NotADirectory => "This is not a folder.".to_owned(),
        ListingError::Unavailable(detail) => format!("This location is unavailable.\n{detail}"),
        ListingError::Other(detail) => format!("Could not open this folder.\n{detail}"),
    }
}
