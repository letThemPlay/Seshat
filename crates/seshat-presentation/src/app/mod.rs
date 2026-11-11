//! Main window component.
//!
//! The window is a shell around the [`WorkspaceService`]: it turns user
//! input into service calls, carries out the I/O the service asks for
//! (listing, watching, launching, saving) and renders the result. Panel and
//! branch state lives in the service, never in widgets.

mod command_line;
mod commands;
mod devices;
mod file_ops;
mod operations;
mod shortcuts;
mod theme;
mod view;
mod workspace;

use command_line::CommandLineController;
pub use command_line::CommandLineMsg;
use devices::DeviceController;
pub use devices::DeviceMsg;
use operations::OperationsController;
pub use operations::OperationsMsg;
use theme::ThemeController;
pub use theme::ThemeMsg;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use seshat_application::interaction::{AppAction, Intent, KeyMap, Mode, format_sequence};
use seshat_application::navigation::{AddressError, breadcrumbs, parse_address};
use seshat_application::operations::{OperationManager, OperationRequest};
use seshat_application::ports::{
    DeviceMonitor, DirectoryChange, DirectoryWatcher, Launcher, ListingError, ThemeRepository,
    WatchGuard,
};
use seshat_application::preferences::{Preferences, PreferencesStore};
use seshat_application::use_cases::ListDirectory;
use seshat_application::workspace::file_actions::Availability;
use seshat_application::workspace::{TransferKind, TransferPlan, WorkspaceService, WorkspaceStore};
use seshat_domain::entities::{FileEntry, Location, StandardLocation};
use seshat_domain::workspace::PanelId;

use crate::actions::{self, ActionContext, Actions};
use crate::clipboard::ClipboardFiles;
use crate::diagnostics;
use crate::keys::{KeyRouter, SharedKeys};
use crate::notice::{self, Notice, Severity};
use crate::operations_panel::OperationsPanel;
use crate::pathbar::address_text;
use crate::sidebar::{Sidebar, SidebarInit, SidebarOutput};

use crate::workspace_view::{WorkspaceEvent, WorkspaceView};
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

/// Workspace changes are saved this long after the last one.
const SAVE_DELAY: Duration = Duration::from_secs(1);

/// Opening many files at once from one action is almost always a mistake.
const MAX_FILES_TO_OPEN: usize = 20;

/// What a feature controller may use from the window while it handles a
/// message. Borrowed from the window's own fields for the duration of one
/// call, so controllers never keep a copy of workspace state.
pub(super) struct Cx<'a> {
    pub service: &'a mut WorkspaceService,
    /// The status bar's transient message.
    pub notice: &'a mut Option<Notice>,
    pub sender: &'a ComponentSender<App>,
}

/// Lends a controller the window's shared state (see [`Cx`]).
macro_rules! cx {
    ($app:expr, $sender:expr) => {
        Cx {
            service: &mut $app.service,
            notice: &mut $app.notice,
            sender: $sender,
        }
    };
}

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
    pub workspace_store: Rc<dyn WorkspaceStore>,
    pub operations: OperationManager,
}

impl std::fmt::Debug for AppInit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppInit").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Msg {
    /// Open a location from outside the tree: sidebar, devices, themes
    /// folder (decision D3).
    Navigate(Location),
    /// A breadcrumb of the active panel.
    Crumb(Location),
    /// Anything keys, menus, the toolbar, shortcuts, mouse buttons and the
    /// command line can ask for, carried out by the single dispatcher
    /// (`App::perform`).
    Perform(AppAction),
    ContextMenu {
        panel: PanelId,
        on_item: bool,
        x: f64,
        y: f64,
    },
    FocusResults,
    FocusFiles,
    CloseSearch,
    Search(String),
    Theme(ThemeMsg),
    CancelAddress,
    AddressSubmitted(String),

    // Workspace
    Workspace(WorkspaceEvent),
    PanelLoaded {
        panel: PanelId,
        generation: u64,
        result: ListingResult,
    },
    PanelChanged {
        panel: PanelId,
        generation: u64,
        change: DirectoryChange,
    },
    ReloadPanel {
        panel: PanelId,
        generation: u64,
    },
    /// A key press, resolved by the modal key handling.
    Intent(Intent),
    /// Keyboard focus moved (e.g. a click or arrow key in a file list).
    FocusMoved,
    CommandLine(CommandLineMsg),
    /// Copy or move the targets into a folder picked from the open panels.
    TransferTo(TransferKind, Location),
    /// A confirmed copy or move across panels.
    SubmitTransfer(TransferPlan),
    SaveWorkspace,

    Device(DeviceMsg),
    OpenFailed(String),
    // File operations
    /// The clipboard was read for a paste.
    PasteFiles(Option<ClipboardFiles>),
    /// Submit a request; select the given item in that panel once listed.
    Submit(OperationRequest, Option<(PanelId, Location)>),
    Operations(OperationsMsg),
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

/// Files waiting on the clipboard to be pasted (decision D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Staged {
    kind: TransferKind,
    count: usize,
}

pub struct App {
    home: PathBuf,
    list_directory: ListDirectory,
    directory_watcher: Rc<dyn DirectoryWatcher>,

    launcher: Rc<dyn Launcher>,
    preferences_store: Rc<dyn PreferencesStore>,
    workspace_store: Rc<dyn WorkspaceStore>,
    sidebar: Controller<Sidebar>,
    actions: Actions,
    ui: Handles,

    service: WorkspaceService,
    keys: SharedKeys,
    command_line: CommandLineController,
    watches: HashMap<PanelId, WatchGuard>,
    /// Panels with a debounced reload scheduled.
    reload_pending: HashSet<PanelId>,
    /// The panel that last had keyboard focus moved to it.
    focused_panel: Option<PanelId>,
    /// Whether that focus went to the file list (not a message page).
    focused_on_files: bool,
    /// Last workspace shape logged (active panel, branches, panels, focus).
    logged_shape: Option<(PanelId, usize, usize, bool)>,
    /// Search text set by the model, so its echo from the entry is ignored.
    search_echo: Option<String>,
    devices: DeviceController,
    preferences: Preferences,
    staged: Option<Staged>,
    /// Transient message for the status bar (errors, device events).
    notice: Option<Notice>,
    editing_address: bool,
    save_pending: bool,

    theme: ThemeController,

    operations: OperationsController,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("active", &self.service.active_location())
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
            .default_width(1180)
            .default_height(720)
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
        diagnostics::start_stall_monitor();

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
                sender
                    .input_sender()
                    .emit(Msg::Theme(ThemeMsg::SystemColorSchemeChanged));
            });
        }

        let devices = DeviceController::new(init.device_monitor);
        devices.connect(&sender);

        let sidebar = Sidebar::builder()
            .launch(SidebarInit {
                places: init.locations,
                devices: devices.devices().to_vec(),
            })
            .forward(sender.input_sender(), |output| match output {
                SidebarOutput::Navigate(location) => Msg::Navigate(location),
                SidebarOutput::Mount(id) => Msg::Device(DeviceMsg::Mount(id)),
                SidebarOutput::Eject(id) => Msg::Device(DeviceMsg::Eject(id)),
            });

        let actions = Actions::new(&sender);
        root.insert_action_group(actions::GROUP, Some(&actions.group));

        let prefs = init.preferences.load();
        let (keymap, key_warnings) =
            KeyMap::new(&prefs.workspace.key_bindings, prefs.workspace.classic_keys);
        for warning in &key_warnings {
            tracing::warn!(warning, "key binding");
        }
        let keys = KeyRouter::new(keymap);

        let workspace_view = Rc::new(WorkspaceView::new(prefs.workspace.layout, {
            let sender = sender.clone();
            move |event| sender.input_sender().emit(Msg::Workspace(event))
        }));
        let panel = OperationsPanel::new({
            let sender = sender.clone();
            move |id| {
                sender
                    .input_sender()
                    .emit(Msg::Operations(OperationsMsg::Cancel(id)))
            }
        });
        let (widgets, ui) = view::build(
            &root,
            sidebar.widget(),
            workspace_view,
            &panel,
            prefs.workspace.classic_keys,
            &sender,
        );
        let ui_theme_button = ui.theme_button.clone();
        let operations = OperationsController::new(init.operations.clone(), panel, root.clone());
        operations.connect(&sender);
        {
            // Keyboard focus decides the cursor: a click or arrow key in a
            // file list moves it there, and activates that panel.
            let sender = sender.clone();
            root.connect_focus_widget_notify(move |_| {
                sender.input_sender().emit(Msg::FocusMoved);
            });
        }
        {
            // Coming back to the window (e.g. after a dialog closes) puts
            // keyboard focus on the files unless a text field had it.
            let sender = sender.clone();
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
                    sender
                        .input_sender()
                        .emit(Msg::Operations(OperationsMsg::WindowHidden));
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
        }
        shortcuts::install(&root, &keys, &ui.command_entry, &sender);

        let defaults = prefs.panel_defaults();
        let restored = if prefs.workspace.restore {
            init.workspace_store.load()
        } else {
            None
        };
        let restored_any = restored.is_some();
        let mut service = match restored {
            Some(workspace) => WorkspaceService::new(workspace, defaults),
            None => WorkspaceService::starting_at(Location::Path(init.home.clone()), defaults),
        };
        service.set_wrap_cursor(prefs.workspace.wrap_cursor);
        service.set_retain_focus_on_branch_switch(prefs.workspace.retain_focus_mode);
        tracing::info!(
            restored = restored_any,
            panels = service.workspace().panel_count(),
            branches = service.workspace().branch_count(),
            "workspace ready"
        );

        let command_line = CommandLineController::new(
            ui.command_entry.clone(),
            init.list_directory.clone(),
            init.home.clone(),
        );
        let mut model = App {
            home: init.home,
            list_directory: init.list_directory,
            directory_watcher: init.directory_watcher,
            launcher: init.launcher,
            preferences_store: init.preferences,
            workspace_store: init.workspace_store,
            sidebar,
            actions,
            ui,
            service,
            keys,
            command_line,
            watches: HashMap::new(),
            reload_pending: HashSet::new(),
            focused_panel: None,
            focused_on_files: false,
            logged_shape: None,
            search_echo: None,
            devices,
            theme: ThemeController::new(
                init.themes,
                prefs.theme.clone(),
                css_provider,
                ui_theme_button,
            ),
            preferences: prefs,
            staged: None,
            notice: key_warnings.first().map(notice::key_bindings),
            editing_address: false,
            save_pending: false,
            operations,
        };
        let watcher = model.directory_watcher.clone();
        model
            .theme
            .start(watcher.as_ref(), &mut cx!(model, &sender));
        model.sync(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        match msg {
            Msg::Navigate(location) => {
                self.editing_address = false;
                self.service.open_location(location);
            }
            Msg::Crumb(location) => self.service.open_ancestor(location),
            Msg::Perform(action) => self.perform(action, &sender),
            Msg::ContextMenu {
                panel,
                on_item,
                x,
                y,
            } => self.context_menu(panel, on_item, x, y),
            Msg::FocusResults => {
                let active = self.service.active_panel();
                if self
                    .service
                    .state(active)
                    .is_some_and(|s| s.cursor.is_none())
                {
                    self.service.cursor_to_first(active);
                }
                self.focus_active();
            }
            Msg::FocusFiles => {
                if !self.typing() {
                    self.focus_active();
                }
            }
            Msg::CloseSearch => {
                if self.ui.search_bar.is_search_mode() {
                    self.ui.search_bar.set_search_mode(false);
                }
            }
            Msg::Search(query) => {
                if self.search_echo.take().as_deref() != Some(query.as_str()) {
                    let active = self.service.active_panel();
                    self.service.set_query(active, &query);
                }
            }
            Msg::Theme(msg) => self.theme_message(msg, &sender),
            Msg::CancelAddress => self.editing_address = false,
            Msg::AddressSubmitted(text) => match self.resolve_address(&text) {
                Ok(location) => {
                    self.editing_address = false;
                    self.service.open_location(location);
                }
                Err(err) => self.notice = Some(Notice::from_error(&err)),
            },

            Msg::Workspace(event) => self.workspace_event(event, &sender),
            Msg::PanelLoaded {
                panel,
                generation,
                result,
            } => self.service.listing_loaded(panel, generation, result.0),
            Msg::PanelChanged {
                panel,
                generation,
                change,
            } => self.panel_changed(panel, generation, change, &sender),
            Msg::ReloadPanel { panel, generation } => {
                self.reload_pending.remove(&panel);
                self.service.reload_if_current(panel, generation);
            }
            Msg::Intent(intent) => self.intent(intent, &sender),
            Msg::FocusMoved => self.focus_moved(),
            Msg::CommandLine(msg) => {
                let outcome = self.command_line.update(msg, &mut cx!(self, &sender));
                self.command_line_outcome(outcome, &sender);
            }
            Msg::TransferTo(kind, destination) => self.plan_transfer(kind, &destination, &sender),
            Msg::SubmitTransfer(plan) => self.submit_transfer(plan),
            Msg::SaveWorkspace => {
                self.save_pending = false;
                self.save_workspace();
            }

            Msg::Device(msg) => {
                // Stop watching panels on a device before ejecting it, so
                // Seshat doesn't keep it busy.
                if let DeviceMsg::Eject(id) = &msg
                    && let Some(root) = self.devices.root_of(id)
                {
                    self.service.detach_device(&root);
                    self.run_effects(&sender);
                }
                if let Some(input) = self.devices.update(msg, &mut cx!(self, &sender)) {
                    self.sidebar.emit(input);
                }
            }
            Msg::OpenFailed(err) => {
                self.notice = Some(notice::open_failed(&err));
            }
            Msg::PasteFiles(files) => self.paste_files(files, &sender),
            Msg::Submit(request, select) => self.submit(request, select),
            Msg::Operations(msg) => {
                let hidden = matches!(msg, OperationsMsg::WindowHidden);
                self.operations.update(msg, &mut cx!(self, &sender));
                if hidden {
                    self.save_workspace();
                }
            }
        }
        self.sync(&sender);
    }

    fn update_view(&self, widgets: &mut AppWidgets, sender: ComponentSender<Self>) {
        let active = self.service.active_panel();
        let current = self.service.active_location();
        let ws = self.service.workspace();
        let state = self.service.state(active).expect("active panel exists");
        self.actions.sync(
            state.options.show_hidden,
            state.mode,
            state.options.sort,
            self.theme.choice(),
            ws.focus_mode(),
            ActionContext {
                files: Availability::of(&self.service),
                branches: ws.branch_count(),
            },
        );
        widgets.back_button.set_sensitive(ws.can_go_back());
        widgets.forward_button.set_sensitive(ws.can_go_forward());
        widgets.up_button.set_sensitive(
            ws.panel(active).and_then(|p| p.parent()).is_some() || current.parent().is_some(),
        );

        if widgets.rendered_crumbs.as_ref() != Some(current) {
            let sender = sender.clone();
            widgets
                .pathbar
                .set_crumbs(&breadcrumbs(current, &self.home), move |target| {
                    sender.input_sender().emit(Msg::Crumb(target));
                });
            widgets.rendered_crumbs = Some(current.clone());
        }
        if self.editing_address != widgets.editing_address {
            widgets.editing_address = self.editing_address;
            if self.editing_address {
                widgets.pathbar.show_entry(&address_text(current));
            } else {
                widgets.pathbar.show_crumbs();
            }
        }

        // Mode and status bar.
        let router = self.keys.borrow();
        let mode = router.mode();
        let pending = format_sequence(router.interaction.pending());
        widgets.mode_label.set_label(mode.label());
        for (class, on) in [
            ("seshat-mode-visual", matches!(mode, Mode::Visual { .. })),
            ("seshat-mode-command", mode == Mode::Command),
        ] {
            if on {
                widgets.mode_label.add_css_class(class);
            } else {
                widgets.mode_label.remove_css_class(class);
            }
        }
        widgets.pending_label.set_label(&pending);
        widgets.focus_label.set_visible(ws.focus_mode());

        let status = self.status_text();
        widgets.status_bar.set_label(&status);
        let severity = self.notice.as_ref().map(Notice::severity);
        for (class, on) in [
            ("seshat-notice", severity.is_some()),
            ("seshat-notice-error", severity == Some(Severity::Error)),
        ] {
            if on {
                widgets.status_bar.add_css_class(class);
            } else {
                widgets.status_bar.remove_css_class(class);
            }
        }
        // Errors are spoken by screen readers once, when they appear
        // (decision N2).
        if let Some(notice) = self.notice.as_ref().filter(|n| n.announce())
            && widgets.announced != Some(notice.id())
        {
            widgets.announced = Some(notice.id());
            widgets
                .status_bar
                .announce(notice.text(), gtk::AccessibleAnnouncementPriority::High);
        }
    }

    fn shutdown(&mut self, _widgets: &mut AppWidgets, _output: relm4::Sender<()>) {
        self.save_workspace();
        // Saves are written in the background; wait for the last one.
        self.workspace_store.flush();
    }
}

impl App {
    fn status_text(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.text().to_owned();
        }
        let selection = self.service.selection();
        if !selection.is_empty() {
            let items: usize = selection.iter().map(|g| g.items.len()).sum();
            let panels = selection.len();
            return match panels {
                1 => format!("{items} selected"),
                n => format!("{items} selected in {n} panels"),
            };
        }
        if let Some(staged) = self.staged {
            return notice::staged(staged.count, staged.kind);
        }
        let active = self.service.active_panel();
        match self.service.listing(active) {
            Some(listing) => notice::item_count(listing.shown().len(), listing.total()),
            None => "Loading…".to_owned(),
        }
    }

    /// Text from the address bar, `:open`, `:copy` or `:move`, resolved
    /// against Home and the active panel's folder.
    fn resolve_address(&self, text: &str) -> Result<Location, AddressError> {
        parse_address(text, &self.home, Some(self.service.active_location()))
    }

    /// A message for the theme controller; saves preferences if the
    /// chosen theme changed.
    fn theme_message(&mut self, msg: ThemeMsg, sender: &ComponentSender<Self>) {
        if self.theme.update(msg, &mut cx!(self, sender)) {
            self.save_preferences();
        }
    }

    /// The context menu from the keyboard (Menu key, Shift+F10), at a
    /// fixed point of the active panel.
    fn keyboard_context_menu(&mut self, sender: &ComponentSender<Self>) {
        let panel = self.service.active_panel();
        let on_item = !self.service.targets().is_empty();
        let size = self
            .ui
            .workspace
            .with_panel(panel, |v| (v.files.root.width(), v.files.root.height()));
        if let Some((w, h)) = size {
            sender.input_sender().emit(Msg::ContextMenu {
                panel,
                on_item,
                x: f64::from(w) / 2.0,
                y: f64::from(h) / 3.0,
            });
        }
    }

    fn typing(&self) -> bool {
        crate::keys::typing_elsewhere(gtk::prelude::GtkWindowExt::focus(&self.ui.window).as_ref())
    }

    /// Moves keyboard focus to the active panel's cursor.
    fn focus_active(&mut self) {
        let active = self.service.active_panel();
        let cursor = self.service.state(active).and_then(|s| s.cursor.clone());
        self.ui.workspace.focus_panel(active, cursor.as_ref());
        self.focused_panel = Some(active);
        self.focused_on_files = self.active_ready();
    }

    fn active_ready(&self) -> bool {
        self.service
            .listing(self.service.active_panel())
            .is_some_and(|l| *l.content() == seshat_application::workspace::Content::Ready)
    }

    fn open_targets(&mut self) {
        let active = self.service.active_panel();
        let entries: Vec<FileEntry> = self
            .service
            .targets()
            .iter()
            .flat_map(|g| {
                let listing = self.service.listing(g.panel);
                g.items
                    .iter()
                    .filter_map(move |l| listing.and_then(|li| li.entry(l)).cloned())
            })
            .collect();
        self.open_entries(active, entries);
    }

    fn open_entries(&mut self, panel: PanelId, entries: Vec<FileEntry>) {
        if let [single] = entries.as_slice()
            && single.is_browsable()
        {
            let _ = self.service.open_entry(panel, single);
            return;
        }
        let files: Vec<Location> = entries
            .into_iter()
            .filter(|e| !e.is_browsable())
            .map(|e| e.location)
            .collect();
        if files.len() > MAX_FILES_TO_OPEN {
            self.notice = Some(notice::too_many_files(MAX_FILES_TO_OPEN));
            return;
        }
        self.launch(files);
    }

    /// Copies the target items' paths (one per line) to the clipboard.
    fn copy_paths(&mut self) {
        let paths: Vec<String> = self
            .service
            .targets()
            .iter()
            .flat_map(|g| g.items.iter().map(address_text))
            .collect();
        if paths.is_empty() {
            return;
        }
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&paths.join("\n"));
            self.notice = Some(notice::copied_paths(paths.len()));
        }
    }

    fn context_menu(&mut self, panel: PanelId, on_item: bool, x: f64, y: f64) {
        let _ = self.service.activate_panel(panel);
        let in_trash = self.service.active_location().is_trash();
        let menu = if on_item {
            actions::item_menu(in_trash)
        } else {
            actions::background_menu(in_trash)
        };
        let popover = &self.ui.context_menu;
        popover.set_menu_model(Some(&menu));
        // Event coordinates are relative to the panel's file view; the
        // popover hangs off the content container.
        let point = self.ui.workspace.with_panel(panel, |v| {
            popover.parent().and_then(|parent| {
                v.files
                    .root
                    .compute_point(&parent, &gtk::graphene::Point::new(x as f32, y as f32))
            })
        });
        let (x, y) = point
            .flatten()
            .map_or((x, y), |p| (f64::from(p.x()), f64::from(p.y())));
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    }

    /// Saves the theme and the defaults for new panels (decision D11: the
    /// last changed panel setting becomes the default).
    fn save_preferences(&mut self) {
        self.preferences.theme = self.theme.choice().clone();
        self.preferences
            .adopt_panel_defaults(self.service.defaults());
        if let Err(err) = self.preferences_store.save(&self.preferences) {
            tracing::warn!(error = %err, "preferences not saved");
        }
    }
}
