//! Main window component.

mod file_ops;
mod shortcuts;
mod view;

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use seshat_application::navigation::{NavigationHistory, breadcrumbs, parse_address};
use seshat_application::operations::{OperationEvent, OperationManager, OperationRequest};
use seshat_application::ports::{
    DeviceError, DeviceMonitor, DirectoryChange, DirectoryWatcher, Launcher, ListingError,
    ThemeRepository, WatchGuard, location_lost,
};
use seshat_application::preferences::{Preferences, PreferencesStore, ThemeChoice};
use seshat_application::use_cases::ListDirectory;
use seshat_application::view::{SortKey, ViewMode, ViewOptions, arrange};
use seshat_domain::entities::{Device, DeviceId, FileEntry, Location, StandardLocation};
use seshat_domain::operation::OperationId;

use crate::actions::{self, ActionContext, Actions};
use crate::clipboard::ClipboardFiles;
use crate::conflicts::{ConflictRequest, UiConflictResolver};
use crate::diagnostics;
use crate::files::{FileView, FileViewEvent};
use crate::operations_panel::OperationsPanel;
use crate::pathbar::address_text;
use crate::sidebar::{Sidebar, SidebarInit, SidebarInput, SidebarOutput};
use crate::theme::{ThemeRegistry, render_css};
use view::{AppWidgets, Handles};

/// Seshat owns its look through its own theme system, so its stylesheet must
/// win over a desktop-wide `~/.config/gtk-4.0/gtk.css` (USER priority);
/// otherwise mandated rules such as the shared surface token and 12px
/// selected radius could be overridden. All rules are scoped to `.seshat*`.
const THEME_PROVIDER_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_USER + 1;

/// Bursts of filesystem events (e.g. extracting an archive) are coalesced
/// into one reload.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(300);
const MAX_RELOAD_DEBOUNCE: Duration = Duration::from_secs(3);

/// Opening many files at once from one action is almost always a mistake.
const MAX_FILES_TO_OPEN: usize = 20;

/// Dependencies supplied by the composition root.
pub struct AppInit {
    pub home: PathBuf,
    pub list_directory: ListDirectory,
    pub locations: Vec<StandardLocation>,
    pub directory_watcher: Rc<dyn DirectoryWatcher>,
    pub device_monitor: Rc<dyn DeviceMonitor>,
    pub launcher: Rc<dyn Launcher>,
    pub preferences: Rc<dyn PreferencesStore>,
    pub themes: Rc<dyn ThemeRepository>,
    pub operations: OperationManager,
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
    OpenSelection,
    CopyPaths,
    /// Context menu from the keyboard (Menu key, Shift+F10).
    KeyboardContextMenu,
    ContextMenu {
        on_item: bool,
        x: f64,
        y: f64,
    },
    SelectionChanged,
    ToggleHidden,
    SetViewMode(String),
    SetSortKey(String),
    ToggleSortDescending,
    StartSearch,
    FocusResults,
    FocusFiles,
    CloseSearch,
    Search(String),
    SetTheme(String),
    ReloadThemes,
    ThemesChanged,
    OpenThemesFolder,
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
    // File operations
    ClipboardStore {
        cut: bool,
    },
    Paste,
    PasteFiles(Option<ClipboardFiles>),
    RenameSelection,
    TrashSelection,
    DeleteSelection,
    RestoreSelection,
    NewItem {
        directory: bool,
    },
    EmptyTrash,
    DropFiles {
        paths: Vec<PathBuf>,
        into: Option<u32>,
        copy: bool,
    },
    /// Submit a request; select `select` once it appears.
    Submit(OperationRequest, Option<Location>),
    Operation(OperationEvent),
    CancelOperation(OperationId),
    Conflict(ConflictRequest),
    ConflictAnswered,
    WindowHidden,
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
    NoResults,
    Error(String),
}

pub struct App {
    home: PathBuf,
    list_directory: ListDirectory,
    directory_watcher: Rc<dyn DirectoryWatcher>,
    device_monitor: Rc<dyn DeviceMonitor>,
    launcher: Rc<dyn Launcher>,
    preferences_store: Rc<dyn PreferencesStore>,
    theme_repository: Rc<dyn ThemeRepository>,
    sidebar: Controller<Sidebar>,
    actions: Actions,
    ui: Handles,

    history: NavigationHistory,
    devices: Vec<Device>,
    /// The current location's full listing; what is shown is derived from
    /// it with `arrange`, so filtering/sorting never re-reads the disk.
    listing: Vec<FileEntry>,
    view: ViewOptions,
    view_mode: ViewMode,
    state: ContentState,
    /// Status bar text derived from the selection.
    selection_summary: Option<String>,
    /// Transient message for the status bar (errors, device events).
    notice: Option<String>,
    editing_address: bool,

    /// Incremented per shown location; stale watcher events are dropped.
    view_generation: u64,
    /// Incremented per listing; stale listing results are dropped.
    load_generation: u64,
    /// Whether the pending listing is a reload of the shown location.
    reloading: bool,
    load_started: Option<Instant>,
    watch: Option<WatchGuard>,
    reload_pending: bool,
    /// How long the last view rebuild took; sets the reload debounce.
    last_rebuild: Duration,

    theme_choice: ThemeChoice,
    themes: ThemeRegistry,
    themes_watch: Option<WatchGuard>,
    themes_reload_pending: bool,
    css_provider: gtk::CssProvider,

    operations: OperationManager,
    /// Conflicts waiting for the open dialog to close.
    conflict_queue: std::collections::VecDeque<ConflictRequest>,
    conflict_dialog: Option<(OperationId, gtk::Window)>,
    /// Keeps the process alive while the window is hidden with work left.
    hold: Option<gtk::gio::ApplicationHoldGuard>,
    /// Select this entry once it shows up (after create/rename).
    pending_select: Option<Location>,
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
                sender.input_sender().emit(Msg::SystemColorSchemeChanged);
            });
        }

        let devices = init.device_monitor.devices();
        init.device_monitor.connect_changed(Box::new({
            let sender = sender.clone();
            move || sender.input_sender().emit(Msg::DevicesChanged)
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

        let actions = Actions::new(&sender);
        root.insert_action_group(actions::GROUP, Some(&actions.group));

        let file_view = FileView::new({
            let sender = sender.clone();
            move |event| match event {
                FileViewEvent::Activate(position) => {
                    sender.input_sender().emit(Msg::Activate(position))
                }
                FileViewEvent::ContextMenu { position, x, y } => {
                    sender.input_sender().emit(Msg::ContextMenu {
                        on_item: position.is_some(),
                        x,
                        y,
                    })
                }
                FileViewEvent::SelectionChanged => {
                    sender.input_sender().emit(Msg::SelectionChanged)
                }
                FileViewEvent::Drop { paths, into, copy } => {
                    sender
                        .input_sender()
                        .emit(Msg::DropFiles { paths, into, copy });
                }
            }
        });
        let panel = OperationsPanel::new({
            let sender = sender.clone();
            move |id| sender.input_sender().emit(Msg::CancelOperation(id))
        });
        let (widgets, ui) = view::build(&root, sidebar.widget(), file_view, panel, &sender);
        init.operations
            .set_conflict_resolver(std::sync::Arc::new(UiConflictResolver::new(
                sender.input_sender().clone(),
            )));
        {
            // Called on worker threads: only forward.
            let input = sender.input_sender().clone();
            init.operations.subscribe(Box::new(move |event| {
                let _ = input.send(Msg::Operation(event.clone()));
            }));
        }
        {
            // Coming back to the window (e.g. after a dialog closes) puts
            // keyboard focus on the files unless a text field had it.
            let sender = sender.clone();
            root.connect_focus_widget_notify(|window| {
                let focus = gtk::prelude::GtkWindowExt::focus(window);
                tracing::debug!(
                    widget = focus.as_ref().map(|w| w.type_().name()).unwrap_or("none"),
                    "keyboard focus"
                );
            });
            root.connect_is_active_notify(move |window| {
                let typing = gtk::prelude::GtkWindowExt::focus(window)
                    .is_some_and(|w| w.is::<gtk::Text>() || w.is::<gtk::Entry>());
                if window.is_active() && !typing {
                    sender.input_sender().emit(Msg::FocusFiles);
                }
            });
        }
        {
            // Closing the window must not abort running operations.
            let operations = init.operations.clone();
            let sender = sender.clone();
            root.connect_close_request(move |window| {
                if operations.active_count() > 0 {
                    window.set_visible(false);
                    sender.input_sender().emit(Msg::WindowHidden);
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
        }
        shortcuts::install(&root, &sender);

        let prefs = init.preferences.load();
        let mut model = App {
            home: init.home,
            list_directory: init.list_directory,
            directory_watcher: init.directory_watcher,
            device_monitor: init.device_monitor,
            launcher: init.launcher,
            preferences_store: init.preferences,
            theme_repository: init.themes,
            sidebar,
            actions,
            ui,
            history: NavigationHistory::default(),
            devices,
            listing: Vec::new(),
            view: ViewOptions {
                show_hidden: prefs.show_hidden,
                sort: prefs.sort,
                query: String::new(),
            },
            view_mode: prefs.view_mode,
            state: ContentState::Loading,
            selection_summary: None,
            notice: None,
            editing_address: false,
            view_generation: 0,
            load_generation: 0,
            reloading: false,
            load_started: None,
            watch: None,
            reload_pending: false,
            last_rebuild: Duration::ZERO,
            theme_choice: prefs.theme,
            themes: ThemeRegistry::default(),
            themes_watch: None,
            themes_reload_pending: false,
            css_provider,
            operations: init.operations,
            conflict_queue: std::collections::VecDeque::new(),
            conflict_dialog: None,
            hold: None,
            pending_select: None,
        };
        model.ui.file_view.set_mode(model.view_mode);
        model.load_themes();
        model.watch_themes(&sender);
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
            Msg::Activate(position) => {
                if let Some(entry) = self.ui.file_view.entry(position) {
                    self.open_entries(vec![entry], &sender);
                }
            }
            Msg::OpenSelection => {
                let selected = self.ui.file_view.selected();
                self.open_entries(selected, &sender);
            }
            Msg::CopyPaths => self.copy_paths(),
            Msg::KeyboardContextMenu => {
                let on_item = !self.ui.file_view.selected().is_empty();
                let root = &self.ui.file_view.root;
                let (x, y) = (
                    f64::from(root.width()) / 2.0,
                    f64::from(root.height()) / 3.0,
                );
                sender
                    .input_sender()
                    .emit(Msg::ContextMenu { on_item, x, y });
            }
            Msg::ContextMenu { on_item, x, y } => {
                let menu = if on_item {
                    actions::item_menu(self.in_trash())
                } else {
                    actions::background_menu(self.in_trash())
                };
                let popover = &self.ui.context_menu;
                popover.set_menu_model(Some(&menu));
                // Event coordinates are relative to the file view; the popover
                // hangs off its container.
                let (x, y) = popover
                    .parent()
                    .and_then(|parent| {
                        self.ui
                            .file_view
                            .root
                            .compute_point(&parent, &gtk::graphene::Point::new(x as f32, y as f32))
                    })
                    .map_or((x, y), |p| (f64::from(p.x()), f64::from(p.y())));
                popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
            Msg::SelectionChanged => self.update_selection_summary(),
            Msg::ToggleHidden => {
                self.view.show_hidden = !self.view.show_hidden;
                self.rearrange(true);
                self.save_preferences();
            }
            Msg::SetViewMode(id) => {
                if let Some(mode) = ViewMode::from_id(&id) {
                    self.view_mode = mode;
                    self.ui.file_view.set_mode(mode);
                    // Keep keyboard focus on the visible view (the hidden one
                    // would swallow shortcuts like Ctrl+A).
                    self.ui.file_view.focus();
                    self.save_preferences();
                }
            }
            Msg::SetSortKey(id) => {
                if let Some(key) = SortKey::from_id(&id) {
                    self.view.sort.key = key;
                    self.rearrange(true);
                    self.save_preferences();
                }
            }
            Msg::ToggleSortDescending => {
                self.view.sort.descending = !self.view.sort.descending;
                self.rearrange(true);
                self.save_preferences();
            }
            Msg::FocusResults => self.ui.file_view.focus_results(),
            Msg::FocusFiles => self.ui.file_view.focus(),
            Msg::CloseSearch => {
                if self.ui.search_bar.is_search_mode() {
                    self.ui.search_bar.set_search_mode(false);
                }
            }
            Msg::StartSearch => {
                self.ui.search_bar.set_search_mode(true);
                self.ui.search_entry.grab_focus();
            }
            Msg::Search(query) => {
                if query != self.view.query {
                    self.view.query = query;
                    self.rearrange(false);
                }
            }
            Msg::SetTheme(id) => {
                if let Some(choice) = ThemeChoice::from_id(&id) {
                    self.theme_choice = choice;
                    self.apply_theme();
                    self.save_preferences();
                }
            }
            Msg::ReloadThemes => {
                self.themes_reload_pending = false;
                self.load_themes();
                self.apply_theme();
            }
            Msg::ThemesChanged if !self.themes_reload_pending => {
                self.themes_reload_pending = true;
                let sender = sender.clone();
                gtk::glib::timeout_add_local_once(RELOAD_DEBOUNCE, move || {
                    sender.input_sender().emit(Msg::ReloadThemes);
                });
            }
            Msg::ThemesChanged => {}
            Msg::OpenThemesFolder => match self.theme_repository.ensure_directory() {
                Ok(directory) => self.go(Location::Path(directory), &sender),
                Err(err) => self.notice = Some(format!("Cannot open the themes folder: {err}")),
            },
            Msg::SystemColorSchemeChanged => {
                if self.theme_choice == ThemeChoice::System {
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
                    self.start_listing(location, true, &sender);
                }
            }
            Msg::DevicesChanged => self.devices_changed(&sender),
            Msg::Mount(id) => {
                let monitor = self.device_monitor.clone();
                let sender = sender.clone();
                relm4::spawn_local(async move {
                    let result = monitor.mount(&id).await.map(Some);
                    sender
                        .input_sender()
                        .emit(Msg::DeviceActionFinished(result));
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
            Msg::OpenFailed(err) => self.notice = Some(format!("Could not open: {err}")),
            Msg::ClipboardStore { cut } => self.clipboard_store(cut),
            Msg::Paste => self.paste(&sender),
            Msg::PasteFiles(files) => self.paste_files(files, &sender),
            Msg::RenameSelection => self.rename_selection(&sender),
            Msg::TrashSelection => self.trash_selection(&sender),
            Msg::DeleteSelection => self.delete_selection(&sender),
            Msg::RestoreSelection => self.restore_selection(&sender),
            Msg::NewItem { directory } => self.new_item(directory, &sender),
            Msg::EmptyTrash => self.empty_trash(&sender),
            Msg::DropFiles { paths, into, copy } => self.drop_files(paths, into, copy, &sender),
            Msg::Submit(request, select) => self.submit(request, select),
            Msg::Operation(event) => self.operation_event(event, &sender),
            Msg::CancelOperation(id) => {
                let _ = self.operations.cancel(id);
            }
            Msg::Conflict(request) => {
                self.conflict_queue.push_back(request);
                self.show_next_conflict(&sender);
            }
            Msg::ConflictAnswered => {
                self.conflict_dialog = None;
                self.show_next_conflict(&sender);
            }
            Msg::WindowHidden => {
                tracing::info!(
                    active = self.operations.active_count(),
                    "window closed; finishing operations in the background"
                );
                self.hold = Some(relm4::main_application().hold());
            }
        }
    }

    fn update_view(&self, widgets: &mut AppWidgets, sender: ComponentSender<Self>) {
        let current = self.history.current();

        self.actions.sync(
            self.view.show_hidden,
            self.view_mode,
            self.view.sort,
            &self.theme_choice,
            ActionContext {
                selected: self.ui.file_view.selected().len(),
                in_trash: self.in_trash(),
                in_folder: matches!(current, Some(Location::Path(_))),
            },
        );
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
                        sender.input_sender().emit(Msg::Navigate(target));
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

        widgets.status_title.remove_css_class("seshat-error");
        let (page, message) = match &self.state {
            ContentState::Loading => ("status", "Loading…".to_owned()),
            ContentState::Empty if current.is_some_and(Location::is_trash) => {
                ("status", "Trash is empty".to_owned())
            }
            ContentState::Empty => ("status", "This folder is empty".to_owned()),
            ContentState::NoResults => (
                "status",
                format!("No items match “{}”", self.view.query.trim()),
            ),
            ContentState::Error(err) => {
                widgets.status_title.add_css_class("seshat-error");
                ("status", err.clone())
            }
            ContentState::Ready => ("files", String::new()),
        };
        widgets.stack.set_visible_child_name(page);
        widgets.status_title.set_label(&message);

        let status = match (&self.notice, &self.selection_summary, &self.state) {
            (Some(notice), _, _) => notice.clone(),
            (None, Some(summary), _) => summary.clone(),
            (None, None, ContentState::Loading) => "Loading…".to_owned(),
            (None, None, ContentState::Error(_)) => "Error".to_owned(),
            (None, None, _) => item_count(self.ui.file_view.len(), self.listing.len()),
        };
        widgets.status_bar.set_label(&status);
        if self.notice.is_some() {
            widgets.status_bar.add_css_class("seshat-notice");
        } else {
            widgets.status_bar.remove_css_class("seshat-notice");
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
    /// listing, search cleared.
    fn show_current(&mut self, sender: &ComponentSender<Self>) {
        let Some(location) = self.history.current().cloned() else {
            return;
        };
        tracing::debug!(?location, "showing location");
        self.view_generation += 1;
        self.reload_pending = false;
        self.notice = None;
        self.selection_summary = None;
        self.state = ContentState::Loading;
        self.listing.clear();
        self.ui.file_view.replace(Vec::new(), false);
        if self.ui.search_bar.is_search_mode() {
            self.ui.search_bar.set_search_mode(false);
        }
        self.ui.search_entry.set_text("");
        self.view.query.clear();

        // Watch before listing so changes between the two aren't missed.
        self.watch = None;
        let view = self.view_generation;
        let watch_sender = sender.clone();
        match self.directory_watcher.watch(
            &location,
            Box::new(move |change| {
                watch_sender
                    .input_sender()
                    .emit(Msg::DirectoryChanged { view, change })
            }),
        ) {
            Ok(guard) => self.watch = Some(guard),
            Err(err) => tracing::debug!(error = %err, "cannot watch location; no live updates"),
        }

        self.sidebar
            .emit(SidebarInput::SetCurrent(Some(location.clone())));
        self.start_listing(location, false, sender);
    }

    /// Starts an asynchronous listing; the result arrives as
    /// [`Msg::Loaded`]. A `reload` keeps selection and scroll position.
    fn start_listing(&mut self, location: Location, reload: bool, sender: &ComponentSender<Self>) {
        self.load_generation += 1;
        self.reloading = reload;
        self.load_started = Some(Instant::now());
        let generation = self.load_generation;
        let use_case = self.list_directory.clone();
        let sender = sender.clone();
        relm4::spawn_local(async move {
            let result = use_case.execute(&location).await;
            sender.input_sender().emit(Msg::Loaded {
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
        match result {
            Ok(entries) => {
                tracing::debug!(count = entries.len(), ?elapsed, "listing loaded");
                self.listing = entries;
                self.state = ContentState::Ready;
                self.rearrange(self.reloading);
                if let Some(location) = self.pending_select.take()
                    && !self.ui.file_view.select_location(&location)
                {
                    // Not shown yet (e.g. filtered or not reloaded): keep waiting.
                    self.pending_select = Some(location);
                }
                if !self.reloading {
                    self.ui.file_view.focus();
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "listing failed");
                self.listing.clear();
                self.ui.file_view.replace(Vec::new(), false);
                self.state = ContentState::Error(error_message(&err));
            }
        }
    }

    /// Re-derives the shown entries from the listing and view options.
    fn rearrange(&mut self, preserve: bool) {
        let initial_load_pending = self.load_started.is_some() && !self.reloading;
        if initial_load_pending || matches!(self.state, ContentState::Error(_)) {
            return; // nothing to arrange yet
        }
        let started = Instant::now();
        let shown = arrange(&self.listing, &self.view);
        let arranged = started.elapsed();
        let count = shown.len();
        self.state = match (shown.is_empty(), self.view.query.trim().is_empty()) {
            (false, _) => ContentState::Ready,
            (true, false) => ContentState::NoResults,
            (true, true) => ContentState::Empty,
        };
        self.ui.file_view.replace(shown, preserve);
        self.update_selection_summary();
        self.last_rebuild = started.elapsed();
        tracing::debug!(count, ?arranged, total = ?self.last_rebuild, "view updated");
    }

    /// Live-reload debounce: at least [`RELOAD_DEBOUNCE`], and three times the
    /// last rebuild so a huge, constantly changing folder (e.g. 100k files
    /// while downloading) cannot keep the UI busy rebuilding.
    fn reload_delay(&self) -> Duration {
        (self.last_rebuild * 3).clamp(RELOAD_DEBOUNCE, MAX_RELOAD_DEBOUNCE)
    }

    fn update_selection_summary(&mut self) {
        let selected = self.ui.file_view.selected();
        self.selection_summary = match selected.len() {
            0 => None,
            n => {
                let bytes: u64 = selected.iter().filter_map(|e| e.size).sum();
                let total = self.ui.file_view.len();
                let size = if bytes > 0 {
                    format!(" ({})", gtk::glib::format_size(bytes))
                } else {
                    String::new()
                };
                Some(format!("{n} of {total} selected{size}"))
            }
        };
    }

    fn open_entries(&mut self, entries: Vec<FileEntry>, sender: &ComponentSender<Self>) {
        if let [single] = entries.as_slice()
            && single.is_browsable()
        {
            self.go(single.location.clone(), sender);
            return;
        }
        let files: Vec<Location> = entries
            .into_iter()
            .filter(|e| !e.is_browsable())
            .map(|e| e.location)
            .collect();
        if files.len() > MAX_FILES_TO_OPEN {
            self.notice = Some(format!(
                "Select at most {MAX_FILES_TO_OPEN} files to open at once."
            ));
            return;
        }
        for location in files {
            let launcher = self.launcher.clone();
            let sender = sender.clone();
            relm4::spawn_local(async move {
                if let Err(err) = launcher.open(&location).await {
                    sender.input_sender().emit(Msg::OpenFailed(err));
                }
            });
        }
    }

    /// Copies the selected entries' paths (one per line) to the clipboard.
    fn copy_paths(&mut self) {
        let paths: Vec<String> = self
            .ui
            .file_view
            .selected()
            .iter()
            .map(|e| address_text(&e.location))
            .collect();
        if paths.is_empty() {
            return;
        }
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&paths.join("\n"));
            self.notice = Some(match paths.len() {
                1 => "Copied path to the clipboard.".to_owned(),
                n => format!("Copied {n} paths to the clipboard."),
            });
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
                gtk::glib::timeout_add_local_once(self.reload_delay(), move || {
                    sender.input_sender().emit(Msg::ReloadNow { view });
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
            sender
                .input_sender()
                .emit(Msg::DeviceActionFinished(result));
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

    fn load_themes(&mut self) {
        self.themes = ThemeRegistry::from_sources(self.theme_repository.load_all());
        self.ui
            .theme_button
            .set_menu_model(Some(&actions::theme_menu(&self.themes)));
        if let Some(error) = self.themes.errors().first() {
            self.notice = Some(format!(
                "Theme “{}” could not be loaded: {}",
                error.id, error.message
            ));
        }
    }

    /// Reloads themes when files in the themes folder change.
    fn watch_themes(&mut self, sender: &ComponentSender<Self>) {
        let directory = match self.theme_repository.ensure_directory() {
            Ok(directory) => directory,
            Err(err) => {
                tracing::debug!(error = %err, "cannot create themes folder; not watching");
                return;
            }
        };
        let sender = sender.clone();
        self.themes_watch = self
            .directory_watcher
            .watch(
                &Location::Path(directory),
                Box::new(move |_| sender.input_sender().emit(Msg::ThemesChanged)),
            )
            .ok();
    }

    fn apply_theme(&mut self) {
        let system_dark = system_prefers_dark();
        let theme = match self.themes.resolve(&self.theme_choice, system_dark) {
            Some(theme) => theme,
            None => {
                self.notice = Some(format!(
                    "Theme “{}” is not available; following the system theme.",
                    self.theme_choice.id().trim_start_matches("custom:")
                ));
                self.themes
                    .resolve(&ThemeChoice::System, system_dark)
                    .expect("built-in themes always resolve")
            }
        };
        if let ThemeChoice::Custom(id) = &self.theme_choice
            && let Some(custom) = self.themes.custom(id)
            && !custom.warnings.is_empty()
        {
            self.notice = Some(format!(
                "Theme “{}” has {} issue(s); see the log for details.",
                custom.theme.name,
                custom.warnings.len()
            ));
        }
        tracing::debug!(theme = %theme.name, choice = ?self.theme_choice, "applying theme");
        self.css_provider.load_from_string(&render_css(&theme));
    }

    fn save_preferences(&self) {
        let prefs = Preferences {
            theme: self.theme_choice.clone(),
            show_hidden: self.view.show_hidden,
            view_mode: self.view_mode,
            sort: self.view.sort,
        };
        if let Err(err) = self.preferences_store.save(&prefs) {
            tracing::warn!(error = %err, "preferences not saved");
        }
    }
}

fn item_count(shown: u32, total: usize) -> String {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let shown = shown as usize;
    if shown == total {
        format!("{total} item{}", plural(total))
    } else {
        format!("{shown} of {total} item{}", plural(total))
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
