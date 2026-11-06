//! Widget construction for the main window.

use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::view::ViewMode;

use super::{App, Msg};
use crate::actions;
use crate::files::FileView;
use crate::pathbar::PathBar;

/// Widgets only touched from `update_view`.
#[derive(Debug)]
pub struct AppWidgets {
    pub back_button: gtk::Button,
    pub forward_button: gtk::Button,
    pub up_button: gtk::Button,
    pub pathbar: PathBar,
    /// Location the breadcrumbs were last rendered for.
    pub rendered_crumbs: Option<seshat_domain::entities::Location>,
    pub editing_address: bool,
    pub stack: gtk::Stack,
    pub status_title: gtk::Label,
    pub status_bar: gtk::Label,
}

/// Widgets the model drives directly from `update`.
#[derive(Debug, Clone)]
pub struct Handles {
    pub file_view: FileView,
    pub theme_button: gtk::MenuButton,
    pub search_bar: gtk::SearchBar,
    pub search_entry: gtk::SearchEntry,
    pub context_menu: gtk::PopoverMenu,
}

fn icon_button(
    icon: &str,
    tooltip: &str,
    sender: &ComponentSender<App>,
    msg: fn() -> Msg,
) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    let sender = sender.clone();
    button.connect_clicked(move |_| sender.input(msg()));
    button
}

fn action_toggle(
    icon: &str,
    tooltip: &str,
    action: &str,
    target: Option<&str>,
) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .action_name(format!("{}.{action}", actions::GROUP))
        .build();
    if let Some(target) = target {
        button.set_action_target_value(Some(&target.to_variant()));
    }
    button
}

pub fn build(
    root: &gtk::Window,
    sidebar: &gtk::ScrolledWindow,
    file_view: FileView,
    sender: &ComponentSender<App>,
) -> (AppWidgets, Handles) {
    // ── Toolbar ──
    let header = gtk::HeaderBar::new();
    header.add_css_class("seshat-toolbar");
    let back_button = icon_button("go-previous-symbolic", "Back (Alt+Left)", sender, || {
        Msg::Back
    });
    let forward_button = icon_button("go-next-symbolic", "Forward (Alt+Right)", sender, || {
        Msg::Forward
    });
    let up_button = icon_button("go-up-symbolic", "Parent folder (Alt+Up)", sender, || {
        Msg::GoUp
    });
    let refresh_button = icon_button("view-refresh-symbolic", "Refresh (F5)", sender, || {
        Msg::Refresh
    });
    header.pack_start(&back_button);
    header.pack_start(&forward_button);
    header.pack_start(&up_button);
    header.pack_start(&refresh_button);

    let pathbar = PathBar::new();
    pathbar.entry.connect_activate({
        let sender = sender.clone();
        move |entry| sender.input(Msg::AddressSubmitted(entry.text().to_string()))
    });
    let escape = gtk::EventControllerKey::new();
    escape.connect_key_pressed({
        let sender = sender.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                sender.input(Msg::CancelAddress);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        }
    });
    pathbar.entry.add_controller(escape);
    header.set_title_widget(Some(&pathbar.root));

    let theme_button = gtk::MenuButton::builder()
        .icon_name("preferences-desktop-appearance-symbolic")
        .tooltip_text("Theme")
        .build();
    let sort_button = gtk::MenuButton::builder()
        .icon_name("view-sort-ascending-symbolic")
        .tooltip_text("Sort")
        .menu_model(&actions::sort_menu())
        .build();
    let view_toggles = gtk::Box::builder().css_classes(["linked"]).build();
    view_toggles.append(&action_toggle(
        "view-list-symbolic",
        "List view (Ctrl+1)",
        "view-mode",
        Some(ViewMode::List.id()),
    ));
    view_toggles.append(&action_toggle(
        "view-grid-symbolic",
        "Grid view (Ctrl+2)",
        "view-mode",
        Some(ViewMode::Grid.id()),
    ));
    let hidden_toggle = action_toggle(
        "view-reveal-symbolic",
        "Show hidden files (Ctrl+H)",
        "show-hidden",
        None,
    );
    let search_toggle = gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search (Ctrl+F)")
        .build();
    let edit_button = icon_button(
        "document-edit-symbolic",
        "Type a location (Ctrl+L)",
        sender,
        || Msg::EditAddress,
    );
    header.pack_end(&theme_button);
    header.pack_end(&sort_button);
    header.pack_end(&view_toggles);
    header.pack_end(&hidden_toggle);
    header.pack_end(&search_toggle);
    header.pack_end(&edit_button);
    root.set_titlebar(Some(&header));

    // ── Search ──
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search this folder")
        .hexpand(true)
        .build();
    let search_bar = gtk::SearchBar::builder()
        .child(&search_entry)
        .show_close_button(true)
        .css_classes(["seshat-search"])
        .build();
    search_bar.connect_entry(&search_entry);
    // Typing in the file list starts a search.
    search_bar.set_key_capture_widget(Some(&file_view.root));
    search_bar
        .bind_property("search-mode-enabled", &search_toggle, "active")
        .bidirectional()
        .sync_create()
        .build();
    search_entry.connect_search_changed({
        let sender = sender.clone();
        move |entry| sender.input(Msg::Search(entry.text().to_string()))
    });
    search_bar.connect_search_mode_enabled_notify({
        let sender = sender.clone();
        move |bar| {
            if !bar.is_search_mode() {
                sender.input(Msg::Search(String::new()));
            }
        }
    });

    // ── Content ──
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
    stack.add_named(&file_view.root, Some("files"));
    stack.add_named(&status_page, Some("status"));

    let context_menu = gtk::PopoverMenu::builder()
        .has_arrow(false)
        .halign(gtk::Align::Start)
        .build();
    context_menu.set_parent(&file_view.root);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["seshat-content"])
        .build();
    content.append(&search_bar);
    content.append(&stack);

    let paned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .start_child(sidebar)
        .end_child(&content)
        .resize_start_child(false)
        .shrink_start_child(false)
        .shrink_end_child(false)
        .position(220)
        .vexpand(true)
        .build();

    let status_bar = gtk::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["seshat-status"])
        .build();

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&paned);
    layout.append(&status_bar);
    root.set_child(Some(&layout));

    (
        AppWidgets {
            back_button,
            forward_button,
            up_button,
            pathbar,
            rendered_crumbs: None,
            editing_address: false,
            stack,
            status_title,
            status_bar,
        },
        Handles {
            file_view,
            theme_button,
            search_bar,
            search_entry,
            context_menu,
        },
    )
}
