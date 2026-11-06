//! Widget construction for the main window.

use gtk::prelude::*;
use relm4::ComponentSender;
use relm4::typed_view::list::TypedListView;

use super::{App, Msg};
use crate::content::FileRow;
use crate::pathbar::PathBar;

#[derive(Debug)]
pub struct AppWidgets {
    pub back_button: gtk::Button,
    pub forward_button: gtk::Button,
    pub up_button: gtk::Button,
    pub pathbar: PathBar,
    /// Location the breadcrumbs were last rendered for.
    pub rendered_crumbs: Option<seshat_domain::entities::Location>,
    pub editing_address: bool,
    pub hidden_toggle: gtk::ToggleButton,
    pub theme_button: gtk::Button,
    pub stack: gtk::Stack,
    pub status_title: gtk::Label,
    pub status_bar: gtk::Label,
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

/// Builds the window content. Returns the widgets plus the parts the model
/// keeps (the file list).
pub fn build(
    root: &gtk::Window,
    sidebar: &gtk::ScrolledWindow,
    entries: &TypedListView<FileRow, gtk::MultiSelection>,
    sender: &ComponentSender<App>,
) -> AppWidgets {
    // Toolbar
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

    let edit_button = icon_button(
        "document-edit-symbolic",
        "Type a location (Ctrl+L)",
        sender,
        || Msg::EditAddress,
    );
    let theme_button = gtk::Button::new();
    theme_button.connect_clicked({
        let sender = sender.clone();
        move |_| sender.input(Msg::CycleTheme)
    });
    let hidden_toggle = gtk::ToggleButton::builder()
        .icon_name("view-reveal-symbolic")
        .tooltip_text("Show hidden files (Ctrl+H)")
        .build();
    hidden_toggle.connect_toggled({
        let sender = sender.clone();
        move |b| sender.input(Msg::ToggleHidden(b.is_active()))
    });
    header.pack_end(&theme_button);
    header.pack_end(&hidden_toggle);
    header.pack_end(&edit_button);
    root.set_titlebar(Some(&header));

    // Content pane
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

    AppWidgets {
        back_button,
        forward_button,
        up_button,
        pathbar,
        rendered_crumbs: None,
        editing_address: false,
        hidden_toggle,
        theme_button,
        stack,
        status_title,
        status_bar,
    }
}
