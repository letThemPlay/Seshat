//! Widget construction for the main window.

use gtk::prelude::*;
use relm4::ComponentSender;
use std::rc::Rc;

use seshat_application::interaction::{Action, AppAction};
use seshat_application::view::ViewMode;
use seshat_domain::Direction;

use super::{App, CommandLineMsg, Msg};
use crate::actions;
use crate::operations_panel::OperationsPanel;
use crate::pathbar::PathBar;
use crate::workspace_view::WorkspaceView;

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
    /// Interaction mode (NORMAL, VISUAL, COMMAND).
    pub mode_label: gtk::Label,
    /// Keys of an unfinished sequence, e.g. `g`.
    pub pending_label: gtk::Label,
    pub focus_label: gtk::Label,
    pub status_bar: gtk::Label,
    /// The last notice announced to screen readers (by `Notice::id`), so a
    /// re-render never repeats an announcement.
    pub announced: Option<u64>,
}

/// Widgets the model drives directly from `update`.
#[derive(Debug, Clone)]
pub struct Handles {
    pub workspace: Rc<WorkspaceView>,
    pub command_entry: gtk::Entry,
    pub input: relm4::Sender<Msg>,
    pub theme_button: gtk::MenuButton,
    pub search_bar: gtk::SearchBar,
    pub search_entry: gtk::SearchEntry,
    pub context_menu: gtk::PopoverMenu,
    pub window: gtk::Window,
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
    button.connect_clicked(move |_| sender.input_sender().emit(msg()));
    button
}

fn action_toggle(icon: &str, tooltip: &str, action: AppAction) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .action_name(actions::detailed_name(action))
        .build();
    if let Some(target) = actions::target(action) {
        button.set_action_target_value(Some(&target.to_variant()));
    }
    button
}

pub fn build(
    root: &gtk::Window,
    sidebar: &gtk::ScrolledWindow,
    workspace: Rc<WorkspaceView>,
    panel: &OperationsPanel,
    classic_keys: bool,
    sender: &ComponentSender<App>,
) -> (AppWidgets, Handles) {
    // ── Toolbar ──
    let header = gtk::HeaderBar::new();
    header.add_css_class("seshat-toolbar");
    let back_button = icon_button(
        "go-previous-symbolic",
        "Back (Alt+Left, Ctrl+O)",
        sender,
        || Msg::Perform(Action::Back.into()),
    );
    let forward_button = icon_button(
        "go-next-symbolic",
        "Forward (Alt+Right, Ctrl+I)",
        sender,
        || Msg::Perform(Action::Forward.into()),
    );
    let up_button = icon_button(
        "go-up-symbolic",
        "Parent panel (Alt+Up, Backspace)",
        sender,
        || Msg::Perform(Action::Parent.into()),
    );
    let refresh_button = icon_button("view-refresh-symbolic", "Refresh (F5)", sender, || {
        Msg::Perform(AppAction::Refresh)
    });
    header.pack_start(&back_button);
    header.pack_start(&forward_button);
    header.pack_start(&up_button);
    header.pack_start(&refresh_button);

    let pathbar = PathBar::new();
    pathbar.entry.connect_activate({
        let sender = sender.clone();
        move |entry| {
            sender
                .input_sender()
                .emit(Msg::AddressSubmitted(entry.text().to_string()))
        }
    });
    let escape = gtk::EventControllerKey::new();
    escape.connect_key_pressed({
        let sender = sender.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                sender.input_sender().emit(Msg::CancelAddress);
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
        AppAction::SetViewMode(ViewMode::List),
    ));
    view_toggles.append(&action_toggle(
        "view-grid-symbolic",
        "Grid view (Ctrl+2)",
        AppAction::SetViewMode(ViewMode::Grid),
    ));
    let hidden_toggle = action_toggle(
        "view-reveal-symbolic",
        "Show hidden files (Ctrl+H)",
        AppAction::ToggleHidden,
    );
    let search_toggle = gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search this panel (/ or Ctrl+F)")
        .build();
    let edit_button = icon_button(
        "document-edit-symbolic",
        "Type a location (Ctrl+L)",
        sender,
        || Msg::Perform(AppAction::EditAddress),
    );
    let focus_toggle = action_toggle(
        "view-fullscreen-symbolic",
        "Focus Mode (zz)",
        Action::FocusMode.into(),
    );
    let branches_button = gtk::Button::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Branches (:branch)")
        .action_name(actions::detailed_name(AppAction::ShowBranches))
        .build();
    header.pack_end(&theme_button);
    header.pack_end(&sort_button);
    header.pack_end(&view_toggles);
    header.pack_end(&hidden_toggle);
    header.pack_end(&search_toggle);
    header.pack_end(&edit_button);
    header.pack_start(&branches_button);
    header.pack_start(&focus_toggle);
    root.set_titlebar(Some(&header));

    // ── Search ──
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search this panel")
        .hexpand(true)
        .build();
    let search_bar = gtk::SearchBar::builder()
        .child(&search_entry)
        .show_close_button(true)
        .css_classes(["seshat-search"])
        .build();
    search_bar.connect_entry(&search_entry);
    // Escape leaves an active search. Both this and the key capture below
    // live on the window: with no results the file list is hidden, and
    // when nothing has focus only the window sees key presses. Text fields
    // (address bar, dialogs) handle their own keys first.
    let escape_search = gtk::EventControllerKey::new();
    escape_search.connect_key_pressed({
        let sender = sender.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                sender.input_sender().emit(Msg::CloseSearch);
            }
            gtk::glib::Propagation::Proceed
        }
    });
    root.add_controller(escape_search);
    // With "classic keys" (decision D1), typing anywhere starts a search.
    // Otherwise letters are key bindings and search starts with / or Ctrl+F.
    if classic_keys {
        search_bar.set_key_capture_widget(Some(root));
    }
    search_bar
        .bind_property("search-mode-enabled", &search_toggle, "active")
        .bidirectional()
        .sync_create()
        .build();
    // Return or Down leaves the search box for the results, so keyboard
    // actions (Ctrl+A, Ctrl+C, Delete…) act on files, not on the search text.
    search_entry.connect_activate({
        let sender = sender.clone();
        move |_| sender.input_sender().emit(Msg::FocusResults)
    });
    let down = gtk::EventControllerKey::new();
    down.connect_key_pressed({
        let sender = sender.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Down {
                sender.input_sender().emit(Msg::FocusResults);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        }
    });
    search_entry.add_controller(down);
    search_entry.connect_search_changed({
        let sender = sender.clone();
        move |entry| {
            sender
                .input_sender()
                .emit(Msg::Search(entry.text().to_string()))
        }
    });
    search_bar.connect_search_mode_enabled_notify({
        let sender = sender.clone();
        move |bar| {
            if !bar.is_search_mode() {
                sender.input_sender().emit(Msg::Search(String::new()));
                // Give the keyboard back to the files, not to nowhere.
                sender.input_sender().emit(Msg::FocusFiles);
            }
        }
    });

    // ── Content ──
    let context_menu = gtk::PopoverMenu::builder()
        .has_arrow(false)
        .halign(gtk::Align::Start)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["seshat-content"])
        .build();
    content.append(&search_bar);
    content.append(&workspace.root);
    // Popovers must hang off a container that unparents arbitrary children
    // on dispose (GtkBox does). GtkStack only removes its pages, so a
    // popover parented to it makes window destruction loop forever.
    context_menu.set_parent(&content);
    // Keyboard users continue in the file list after the menu closes.
    context_menu.connect_closed({
        let sender = sender.clone();
        move |_| sender.input_sender().emit(Msg::FocusFiles)
    });

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

    // ── Mode and status bar ──
    let mode_label = gtk::Label::builder()
        .label("NORMAL")
        .css_classes(["seshat-mode"])
        .build();
    mode_label.update_property(&[gtk::accessible::Property::Label("Interaction mode")]);
    let pending_label = gtk::Label::builder()
        .css_classes(["seshat-pending", "dim-label"])
        .build();
    let command_entry = command_line(sender);
    let status_bar = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let focus_label = gtk::Label::builder()
        .label("FOCUS")
        .css_classes(["seshat-focus-indicator"])
        .tooltip_text("Focus Mode: zz or the toolbar button to leave")
        .visible(false)
        .build();
    let status = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .css_classes(["seshat-status"])
        .build();
    status.append(&mode_label);
    status.append(&command_entry);
    status.append(&status_bar);
    status.append(&pending_label);
    status.append(&focus_label);

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&paned);
    layout.append(&panel.root);
    layout.append(&status);
    root.set_child(Some(&layout));

    (
        AppWidgets {
            back_button,
            forward_button,
            up_button,
            pathbar,
            rendered_crumbs: None,
            editing_address: false,
            mode_label,
            pending_label,
            focus_label,
            status_bar,
            announced: None,
        },
        Handles {
            workspace,
            command_entry,
            input: sender.input_sender().clone(),
            theme_button,
            search_bar,
            search_entry,
            context_menu,
            window: root.clone(),
        },
    )
}

/// The command line (`:`): Return runs, Esc cancels, Up/Down recall
/// history, Tab completes. If focus leaves it, the next key press closes it
/// (see `shortcuts::install_modal_keys`), so keys never go to a forgotten
/// prompt.
fn command_line(sender: &ComponentSender<App>) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .placeholder_text(":command (Tab completes, :help lists commands)")
        .width_chars(36)
        .css_classes(["seshat-command"])
        .visible(false)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label("Command")]);
    entry.connect_activate({
        let sender = sender.clone();
        move |entry| {
            sender
                .input_sender()
                .emit(Msg::CommandLine(CommandLineMsg::Submitted(
                    entry.text().to_string(),
                )))
        }
    });
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let sender = sender.clone();
        move |_, key, _, _| {
            let msg = match key {
                gtk::gdk::Key::Escape => CommandLineMsg::Cancelled,
                gtk::gdk::Key::Up => CommandLineMsg::History { older: true },
                gtk::gdk::Key::Down => CommandLineMsg::History { older: false },
                gtk::gdk::Key::Tab => CommandLineMsg::Complete(Direction::Next),
                gtk::gdk::Key::ISO_Left_Tab => CommandLineMsg::Complete(Direction::Previous),
                _ => return gtk::glib::Propagation::Proceed,
            };
            sender.input_sender().emit(Msg::CommandLine(msg));
            gtk::glib::Propagation::Stop
        }
    });
    entry.add_controller(keys);
    entry
}
