//! Main window component.

use std::time::Instant;

use gtk::prelude::*;
use relm4::typed_view::list::TypedListView;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use seshat_application::ports::ListingError;
use seshat_application::use_cases::{ListDirectory, ListingOptions};
use seshat_domain::entities::{FileEntry, Location, StandardLocation};

use crate::content::FileRow;
use crate::diagnostics;
use crate::sidebar;
use crate::theme::{ThemePreference, render_css};

/// Seshat owns its look through its own theme system, so its stylesheet must
/// win over a desktop-wide `~/.config/gtk-4.0/gtk.css` (USER priority);
/// otherwise mandated rules such as the shared surface token and 12px
/// selected radius could be overridden. All rules are scoped to `.seshat*`.
const THEME_PROVIDER_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_USER + 1;

/// Dependencies supplied by the composition root.
#[derive(Debug)]
pub struct AppInit {
    pub list_directory: ListDirectory,
    pub locations: Vec<StandardLocation>,
}

#[derive(Debug)]
pub enum Msg {
    SelectPlace(usize),
    Activate(u32),
    GoUp,
    Refresh,
    ToggleHidden(bool),
    CycleTheme,
    SystemColorSchemeChanged,
    Loaded {
        generation: u64,
        result: ListingResult,
    },
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
    list_directory: ListDirectory,
    locations: Vec<StandardLocation>,
    current: Option<Location>,
    show_hidden: bool,
    state: ContentState,
    entries: TypedListView<FileRow, gtk::MultiSelection>,
    /// Incremented per load so results of superseded loads are dropped.
    generation: u64,
    load_started: Option<Instant>,
    theme_preference: ThemePreference,
    css_provider: gtk::CssProvider,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("current", &self.current)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct AppWidgets {
    stack: gtk::Stack,
    status_title: gtk::Label,
    location_label: gtk::Label,
    up_button: gtk::Button,
    theme_button: gtk::Button,
    status_bar: gtk::Label,
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

        // Toolbar
        let header = gtk::HeaderBar::new();
        header.add_css_class("seshat-toolbar");
        let up_button = gtk::Button::from_icon_name("go-up-symbolic");
        up_button.set_tooltip_text(Some("Parent folder"));
        up_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(Msg::GoUp)
        });
        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some("Refresh"));
        refresh_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(Msg::Refresh)
        });
        header.pack_start(&up_button);
        header.pack_start(&refresh_button);

        let location_label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .css_classes(["seshat-path"])
            .build();
        header.set_title_widget(Some(&location_label));

        let theme_button = gtk::Button::new();
        theme_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(Msg::CycleTheme)
        });
        let hidden_toggle = gtk::ToggleButton::builder()
            .icon_name("view-reveal-symbolic")
            .tooltip_text("Show hidden files")
            .build();
        hidden_toggle.connect_toggled({
            let sender = sender.clone();
            move |b| sender.input(Msg::ToggleHidden(b.is_active()))
        });
        header.pack_end(&theme_button);
        header.pack_end(&hidden_toggle);
        root.set_titlebar(Some(&header));

        // Navigation pane
        let places = sidebar::build(&init.locations);
        places.connect_row_selected({
            let sender = sender.clone();
            move |_, row| {
                if let Some(index) = row.and_then(|r| usize::try_from(r.index()).ok()) {
                    sender.input(Msg::SelectPlace(index));
                }
            }
        });
        let sidebar_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&places)
            .css_classes(["seshat-sidebar"])
            .build();

        // Content pane
        let entries: TypedListView<FileRow, gtk::MultiSelection> = TypedListView::new();
        entries
            .view
            .update_property(&[gtk::accessible::Property::Label("Files")]);
        entries.view.connect_activate({
            let sender = sender.clone();
            move |_, position| sender.input(Msg::Activate(position))
        });
        let list_scroll = gtk::ScrolledWindow::builder()
            .child(&entries.view)
            .vexpand(true)
            .build();

        let status_title = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        let status_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .build();
        status_page.append(&status_title);

        let stack = gtk::Stack::new();
        stack.add_named(&list_scroll, Some("list"));
        stack.add_named(&status_page, Some("status"));

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["seshat-content"])
            .build();
        content.append(&stack);

        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&sidebar_scroll)
            .end_child(&content)
            .resize_start_child(false)
            .shrink_start_child(false)
            .shrink_end_child(false)
            .position(220)
            .vexpand(true)
            .build();

        let status_bar = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["seshat-status"])
            .build();

        let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
        layout.append(&paned);
        layout.append(&status_bar);
        root.set_child(Some(&layout));

        let model = App {
            list_directory: init.list_directory,
            locations: init.locations,
            current: None,
            show_hidden: false,
            state: ContentState::Loading,
            entries,
            generation: 0,
            load_started: None,
            theme_preference: ThemePreference::System,
            css_provider,
        };
        model.apply_theme();

        // Selecting the first place (Home) triggers the initial load.
        if let Some(first) = places.row_at_index(0) {
            places.select_row(Some(&first));
        }

        let widgets = AppWidgets {
            stack,
            status_title,
            location_label,
            up_button,
            theme_button,
            status_bar,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Msg, sender: ComponentSender<Self>) {
        match msg {
            Msg::SelectPlace(index) => {
                // GTK can report the same selection more than once; selecting
                // the current place is not a reload (that's Refresh).
                if let Some(place) = self.locations.get(index)
                    && self.current.as_ref() != Some(&place.location)
                {
                    self.navigate(place.location.clone(), &sender);
                }
            }
            Msg::Activate(position) => {
                let target = self.entries.get(position).and_then(|item| {
                    let row = item.borrow();
                    row.0.is_directory().then(|| row.0.path.clone())
                });
                if let Some(path) = target {
                    self.navigate(Location::Path(path), &sender);
                }
            }
            Msg::GoUp => {
                if let Some(Location::Path(path)) = &self.current
                    && let Some(parent) = path.parent()
                {
                    self.navigate(Location::Path(parent.to_path_buf()), &sender);
                }
            }
            Msg::Refresh => {
                if let Some(location) = self.current.clone() {
                    self.navigate(location, &sender);
                }
            }
            Msg::ToggleHidden(show) => {
                self.show_hidden = show;
                if let Some(location) = self.current.clone() {
                    self.navigate(location, &sender);
                }
            }
            Msg::CycleTheme => {
                self.theme_preference = self.theme_preference.next();
                self.apply_theme();
            }
            Msg::SystemColorSchemeChanged => {
                if self.theme_preference == ThemePreference::System {
                    self.apply_theme();
                }
            }
            Msg::Loaded { generation, result } => self.finish_load(generation, result.0),
        }
    }

    fn update_view(&self, widgets: &mut AppWidgets, _sender: ComponentSender<Self>) {
        let location_text = match &self.current {
            Some(Location::Path(path)) => path.display().to_string(),
            Some(Location::Trash) => "Trash".to_owned(),
            None => String::new(),
        };
        widgets.location_label.set_label(&location_text);
        widgets.up_button.set_sensitive(
            matches!(&self.current, Some(Location::Path(p)) if p.parent().is_some()),
        );
        widgets
            .theme_button
            .set_label(&format!("Theme: {}", self.theme_preference.label()));

        widgets.status_title.remove_css_class("seshat-error");
        let (page, message, status) = match &self.state {
            ContentState::Loading => ("status", "Loading…".to_owned(), "Loading…".to_owned()),
            ContentState::Empty => (
                "status",
                "This folder is empty".to_owned(),
                "0 items".to_owned(),
            ),
            ContentState::Error(err) => {
                widgets.status_title.add_css_class("seshat-error");
                ("status", err.clone(), "Error".to_owned())
            }
            ContentState::Ready => {
                let count = self.entries.len();
                (
                    "list",
                    String::new(),
                    format!("{count} item{}", if count == 1 { "" } else { "s" }),
                )
            }
        };
        widgets.stack.set_visible_child_name(page);
        widgets.status_title.set_label(&message);
        widgets.status_bar.set_label(&status);
    }
}

impl App {
    /// Starts an asynchronous listing of `location`. The UI keeps running;
    /// the result arrives later as [`Msg::Loaded`].
    fn navigate(&mut self, location: Location, sender: &ComponentSender<Self>) {
        self.generation += 1;
        tracing::debug!(generation = self.generation, ?location, "navigating");
        self.current = Some(location.clone());
        self.state = ContentState::Loading;
        self.entries.clear();
        self.load_started = Some(Instant::now());

        let generation = self.generation;
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
        if generation != self.generation {
            return; // superseded by a newer navigation
        }
        let elapsed = self.load_started.take().map(|t| t.elapsed());
        match result {
            Ok(entries) => {
                tracing::debug!(count = entries.len(), ?elapsed, location = ?self.current, "listing loaded");
                self.state = if entries.is_empty() {
                    ContentState::Empty
                } else {
                    ContentState::Ready
                };
                self.entries
                    .extend_from_iter(entries.into_iter().map(FileRow));
            }
            Err(err) => {
                tracing::warn!(error = %err, location = ?self.current, "listing failed");
                self.state = ContentState::Error(error_message(&err));
            }
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
