//! Main window component.
//!
//! The window is a shell around the [`WorkspaceService`]: it turns user
//! input into service calls, carries out the I/O the service asks for
//! (listing, watching, launching, saving) and renders the result. Panel and
//! branch state lives in the service, never in widgets.

mod commands;
mod file_ops;
mod shortcuts;
mod view;
mod workspace;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use seshat_application::interaction::command::CommandHistory;
use seshat_application::interaction::completion::CompletionCycle;
use seshat_application::interaction::{Intent, KeyMap, Mode, format_sequence};
use seshat_application::navigation::{breadcrumbs, parse_address};
use seshat_application::operations::{OperationEvent, OperationManager, OperationRequest};
use seshat_application::ports::{
    DeviceError, DeviceMonitor, DirectoryChange, DirectoryWatcher, Launcher, ListingError,
    ThemeRepository, WatchGuard,
};
use seshat_application::preferences::{
    Preferences, PreferencesStore, ThemeChoice, WorkspaceLayout,
};
use seshat_application::use_cases::ListDirectory;
use seshat_application::view::{SortKey, ViewMode};
use seshat_application::workspace::{TransferKind, TransferPlan, WorkspaceService, WorkspaceStore};
use seshat_domain::entities::{Device, DeviceId, FileEntry, Location, StandardLocation};
use seshat_domain::operation::OperationId;
use seshat_domain::view::ViewOptions;
use seshat_domain::workspace::{PanelId, PanelState};

use crate::actions::{self, ActionContext, Actions};
use crate::clipboard::ClipboardFiles;
use crate::conflicts::{ConflictRequest, UiConflictResolver};
use crate::diagnostics;
use crate::keys::{KeyRouter, SharedKeys};
use crate::operations_panel::OperationsPanel;
use crate::pathbar::address_text;
use crate::sidebar::{Sidebar, SidebarInit, SidebarInput, SidebarOutput};
use crate::theme::{ThemeRegistry, render_css};
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
    Back,
    Forward,
    GoUp,
    Refresh,
    OpenSelection,
    CopyPaths,
    /// Context menu from the keyboard (Menu key, Shift+F10).
    KeyboardContextMenu,
    ContextMenu {
        panel: PanelId,
        on_item: bool,
        x: f64,
        y: f64,
    },
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
    CycleBranch(bool),
    CloseBranch,
    ToggleFocusMode,
    /// Keyboard focus moved (e.g. a click or arrow key in a file list).
    FocusMoved,
    CommandSubmitted(String),
    CommandCancelled,
    CommandHistory {
        older: bool,
    },
    /// Tab (`forward`) or Shift+Tab in the command line.
    CommandComplete {
        forward: bool,
    },
    /// A folder listed for path completion.
    CompletionListed {
        request: u64,
        text: String,
        keep: String,
        partial: String,
        result: ListingResult,
    },
    ShowHelp,
    ShowBranches,
    /// Copy or move the targets into a folder picked from the open panels.
    TransferTo(TransferKind, Location),
    /// A confirmed copy or move across panels.
    SubmitTransfer(TransferPlan),
    SaveWorkspace,

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
    /// Confirmed Trash of items from several panels.
    TrashPaths(Vec<PathBuf>),
    DeleteSelection,
    RestoreSelection,
    NewItem {
        directory: bool,
    },
    EmptyTrash,
    /// Submit a request; select the given item in that panel once listed.
    Submit(OperationRequest, Option<(PanelId, Location)>),
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

/// Files waiting on the clipboard to be pasted (decision D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Staged {
    cut: bool,
    count: usize,
}

pub struct App {
    home: PathBuf,
    list_directory: ListDirectory,
    directory_watcher: Rc<dyn DirectoryWatcher>,
    device_monitor: Rc<dyn DeviceMonitor>,
    launcher: Rc<dyn Launcher>,
    preferences_store: Rc<dyn PreferencesStore>,
    theme_repository: Rc<dyn ThemeRepository>,
    workspace_store: Rc<dyn WorkspaceStore>,
    sidebar: Controller<Sidebar>,
    actions: Actions,
    ui: Handles,

    service: WorkspaceService,
    keys: SharedKeys,
    command_history: CommandHistory,
    /// Tab cycling through completion matches (decision P1).
    completion: Option<CompletionCycle>,
    /// Numbers background listings for completion; late ones are dropped.
    completion_request: u64,
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
    devices: Vec<Device>,
    preferences: Preferences,
    staged: Option<Staged>,
    /// Transient message for the status bar (errors, device events).
    notice: Option<String>,
    editing_address: bool,
    save_pending: bool,

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

        let prefs = init.preferences.load();
        let (keymap, key_warnings) =
            KeyMap::new(&prefs.workspace.key_bindings, prefs.workspace.classic_keys);
        for warning in &key_warnings {
            tracing::warn!(warning, "key binding");
        }
        let keys = KeyRouter::new(keymap);

        let strip = prefs.workspace.layout == WorkspaceLayout::Strip;
        let workspace_view = Rc::new(WorkspaceView::new(strip, {
            let sender = sender.clone();
            move |event| sender.input_sender().emit(Msg::Workspace(event))
        }));
        let panel = OperationsPanel::new({
            let sender = sender.clone();
            move |id| sender.input_sender().emit(Msg::CancelOperation(id))
        });
        let (widgets, ui) = view::build(
            &root,
            sidebar.widget(),
            workspace_view,
            panel,
            prefs.workspace.classic_keys,
            &sender,
        );
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
                    sender.input_sender().emit(Msg::WindowHidden);
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
        }
        shortcuts::install(&root, &keys, &ui.command_entry, &sender);

        let defaults = PanelState {
            options: ViewOptions {
                show_hidden: prefs.show_hidden,
                sort: prefs.sort,
                query: String::new(),
            },
            mode: prefs.view_mode,
            ..PanelState::default()
        };
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

        let mut model = App {
            home: init.home,
            list_directory: init.list_directory,
            directory_watcher: init.directory_watcher,
            device_monitor: init.device_monitor,
            launcher: init.launcher,
            preferences_store: init.preferences,
            theme_repository: init.themes,
            workspace_store: init.workspace_store,
            sidebar,
            actions,
            ui,
            service,
            keys,
            command_history: CommandHistory::default(),
            completion: None,
            completion_request: 0,
            watches: HashMap::new(),
            reload_pending: HashSet::new(),
            focused_panel: None,
            focused_on_files: false,
            logged_shape: None,
            search_echo: None,
            devices,
            theme_choice: prefs.theme.clone(),
            preferences: prefs,
            staged: None,
            notice: key_warnings
                .first()
                .map(|w| format!("Key bindings: {w} (see the log)")),
            editing_address: false,
            save_pending: false,
            themes: ThemeRegistry::default(),
            themes_watch: None,
            themes_reload_pending: false,
            css_provider,
            operations: init.operations,
            conflict_queue: std::collections::VecDeque::new(),
            conflict_dialog: None,
            hold: None,
        };
        model.load_themes();
        model.watch_themes(&sender);
        model.apply_theme();
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
            Msg::Back => {
                self.service.go_back();
            }
            Msg::Forward => {
                self.service.go_forward();
            }
            Msg::GoUp => {
                self.service.go_parent();
            }
            Msg::Refresh => {
                let active = self.service.active_panel();
                self.service.reload(active);
            }
            Msg::OpenSelection => self.open_targets(),
            Msg::CopyPaths => self.copy_paths(),
            Msg::KeyboardContextMenu => {
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
            Msg::ContextMenu {
                panel,
                on_item,
                x,
                y,
            } => self.context_menu(panel, on_item, x, y),
            Msg::ToggleHidden => {
                let active = self.service.active_panel();
                self.service.toggle_hidden(active);
                self.save_preferences();
            }
            Msg::SetViewMode(id) => {
                if let Some(mode) = ViewMode::from_id(&id) {
                    let active = self.service.active_panel();
                    self.service.set_view_mode(active, mode);
                    self.save_preferences();
                    self.focused_panel = None; // the visible view changed
                }
            }
            Msg::SetSortKey(id) => {
                if let Some(key) = SortKey::from_id(&id) {
                    let active = self.service.active_panel();
                    self.service.set_sort_key(active, key);
                    self.save_preferences();
                }
            }
            Msg::ToggleSortDescending => {
                let active = self.service.active_panel();
                self.service.toggle_sort_descending(active);
                self.save_preferences();
            }
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
            Msg::StartSearch => self.start_search(),
            Msg::Search(query) => {
                if self.search_echo.take().as_deref() != Some(query.as_str()) {
                    let active = self.service.active_panel();
                    self.service.set_query(active, &query);
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
                Ok(directory) => self.service.open_location(Location::Path(directory)),
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
                let current = self.service.active_location().clone();
                match parse_address(&text, &self.home, Some(&current)) {
                    Ok(location) => {
                        self.editing_address = false;
                        self.service.open_location(location);
                    }
                    Err(err) => self.notice = Some(err.to_string()),
                }
            }

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
            Msg::CycleBranch(forward) => {
                self.service.cycle_branch(forward);
            }
            Msg::CloseBranch => self.close_branch(None),
            Msg::ToggleFocusMode => {
                self.service.toggle_focus_mode();
            }
            Msg::FocusMoved => self.focus_moved(),
            Msg::CommandSubmitted(text) => self.command_submitted(&text, &sender),
            Msg::CommandCancelled => self.close_command_line(),
            Msg::CommandHistory { older } => self.command_history_step(older),
            Msg::CommandComplete { forward } => self.complete_command(forward, &sender),
            Msg::CompletionListed {
                request,
                text,
                keep,
                partial,
                result,
            } => self.completion_listed(request, &text, keep, &partial, result.0),
            Msg::ShowHelp => self.show_help(),
            Msg::ShowBranches => self.show_branches(),
            Msg::TransferTo(kind, destination) => self.plan_transfer(kind, &destination, &sender),
            Msg::SubmitTransfer(plan) => self.submit_transfer(plan),
            Msg::SaveWorkspace => {
                self.save_pending = false;
                self.save_workspace();
            }

            Msg::DevicesChanged => self.devices_changed(),
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
                Ok(Some(root)) => self.service.open_location(Location::Path(root)),
                Ok(None) => self.notice = Some("The device can be removed safely.".to_owned()),
                Err(err) => {
                    tracing::warn!(error = %err, "device action failed");
                    self.notice = Some(format!("Device action failed: {err}"));
                    // Reselect the current location instead of the device row.
                    self.sidebar.emit(SidebarInput::SetCurrent(Some(
                        self.service.active_location().clone(),
                    )));
                }
            },
            Msg::OpenFailed(err) => self.notice = Some(format!("Could not open: {err}")),
            Msg::ClipboardStore { cut } => self.clipboard_store(cut),
            Msg::Paste => self.paste(&sender),
            Msg::PasteFiles(files) => self.paste_files(files, &sender),
            Msg::RenameSelection => self.rename_selection(&sender),
            Msg::TrashSelection => self.trash_selection(&sender),
            Msg::TrashPaths(paths) => self.submit(OperationRequest::Trash { paths }, None),
            Msg::DeleteSelection => self.delete_selection(&sender),
            Msg::RestoreSelection => self.restore_selection(),
            Msg::NewItem { directory } => self.new_item(directory, None, &sender),
            Msg::EmptyTrash => self.empty_trash(&sender),
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
                self.save_workspace();
                self.hold = Some(relm4::main_application().hold());
            }
        }
        self.sync(&sender);
    }

    fn update_view(&self, widgets: &mut AppWidgets, sender: ComponentSender<Self>) {
        let active = self.service.active_panel();
        let current = self.service.active_location();
        let ws = self.service.workspace();
        let state = self.service.state(active).expect("active panel exists");
        let targets: usize = self.service.targets().iter().map(|g| g.items.len()).sum();

        self.actions.sync(
            state.options.show_hidden,
            state.mode,
            state.options.sort,
            &self.theme_choice,
            ws.focus_mode(),
            ActionContext {
                selected: targets,
                in_trash: current.is_trash(),
                in_folder: matches!(current, Location::Path(_)),
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
        if self.notice.is_some() {
            widgets.status_bar.add_css_class("seshat-notice");
        } else {
            widgets.status_bar.remove_css_class("seshat-notice");
        }
    }

    fn shutdown(&mut self, _widgets: &mut AppWidgets, _output: relm4::Sender<()>) {
        self.save_workspace();
    }
}

impl App {
    fn status_text(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
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
            let what = if staged.count == 1 {
                "1 item".to_owned()
            } else {
                format!("{} items", staged.count)
            };
            let verb = if staged.cut { "move" } else { "copy" };
            return format!("{what} ready to {verb}: paste (p or Ctrl+V) into a panel");
        }
        let active = self.service.active_panel();
        match self.service.listing(active) {
            Some(listing) => item_count(listing.shown().len(), listing.total()),
            None => "Loading…".to_owned(),
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
            self.notice = Some(format!(
                "Select at most {MAX_FILES_TO_OPEN} files to open at once."
            ));
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
            self.notice = Some(match paths.len() {
                1 => "Copied path to the clipboard.".to_owned(),
                n => format!("Copied {n} paths to the clipboard."),
            });
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

    fn devices_changed(&mut self) {
        let devices = self.device_monitor.devices();
        let vanished = seshat_application::ports::vanished_roots(&self.devices, &devices);
        tracing::debug!(
            count = devices.len(),
            vanished = vanished.len(),
            "devices changed"
        );
        self.devices = devices.clone();
        self.sidebar.emit(SidebarInput::SetDevices(devices));
        let lost: usize = vanished
            .iter()
            .map(|root| self.service.detach_device(root))
            .sum();
        if lost > 0 {
            self.notice = Some(
                "The device was removed. Its panels are kept until you close them.".to_owned(),
            );
        }
    }

    fn eject(&mut self, id: DeviceId, sender: &ComponentSender<Self>) {
        // Stop watching panels on the device first, so we don't keep it busy.
        let root = self
            .devices
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.root().map(PathBuf::from));
        if let Some(root) = root {
            self.service.detach_device(&root);
            self.run_effects(sender);
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

    /// Saves the theme and the defaults for new panels (decision D11: the
    /// last changed panel setting becomes the default).
    fn save_preferences(&mut self) {
        let defaults = self.service.defaults();
        self.preferences.theme = self.theme_choice.clone();
        self.preferences.show_hidden = defaults.options.show_hidden;
        self.preferences.view_mode = defaults.mode;
        self.preferences.sort = defaults.options.sort;
        if let Err(err) = self.preferences_store.save(&self.preferences) {
            tracing::warn!(error = %err, "preferences not saved");
        }
    }
}

fn item_count(shown: usize, total: usize) -> String {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
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
