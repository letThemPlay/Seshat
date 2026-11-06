//! Window actions and the menus built on them.
//!
//! Stateful actions (hidden files, view mode, sort, theme) are the single
//! source the toolbar toggles, menus and context menus render from, so they
//! can never disagree.

use gtk::gio;
use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::preferences::ThemeChoice;
use seshat_application::view::{SortKey, SortOrder, ViewMode};

use crate::app::{App, Msg};
use crate::theme::ThemeRegistry;

pub const GROUP: &str = "seshat";

#[derive(Debug, Clone)]
pub struct Actions {
    pub group: gio::SimpleActionGroup,
    show_hidden: gio::SimpleAction,
    view_mode: gio::SimpleAction,
    sort_key: gio::SimpleAction,
    sort_descending: gio::SimpleAction,
    theme: gio::SimpleAction,
}

impl Actions {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let group = gio::SimpleActionGroup::new();

        let simple = |name: &str, msg: fn() -> Msg| {
            let action = gio::SimpleAction::new(name, None);
            let sender = sender.clone();
            action.connect_activate(move |_, _| sender.input(msg()));
            group.add_action(&action);
        };
        simple("open", || Msg::OpenSelection);
        simple("copy-path", || Msg::CopyPaths);
        simple("refresh", || Msg::Refresh);
        simple("reload-themes", || Msg::ReloadThemes);
        simple("open-themes-folder", || Msg::OpenThemesFolder);

        let show_hidden = gio::SimpleAction::new_stateful("show-hidden", None, &false.to_variant());
        show_hidden.connect_activate({
            let sender = sender.clone();
            move |_, _| sender.input(Msg::ToggleHidden)
        });
        let sort_descending =
            gio::SimpleAction::new_stateful("sort-descending", None, &false.to_variant());
        sort_descending.connect_activate({
            let sender = sender.clone();
            move |_, _| sender.input(Msg::ToggleSortDescending)
        });

        let string_action = |name: &str, initial: &str, msg: fn(String) -> Msg| {
            let action = gio::SimpleAction::new_stateful(
                name,
                Some(&String::static_variant_type()),
                &initial.to_variant(),
            );
            let sender = sender.clone();
            action.connect_activate(move |_, param| {
                if let Some(value) = param.and_then(|p| p.get::<String>()) {
                    sender.input(msg(value));
                }
            });
            action
        };
        let view_mode = string_action("view-mode", ViewMode::List.id(), Msg::SetViewMode);
        let sort_key = string_action("sort-key", SortKey::Name.id(), Msg::SetSortKey);
        let theme = string_action("theme", &ThemeChoice::System.id(), Msg::SetTheme);

        for action in [
            &show_hidden,
            &sort_descending,
            &view_mode,
            &sort_key,
            &theme,
        ] {
            group.add_action(action);
        }

        Self {
            group,
            show_hidden,
            view_mode,
            sort_key,
            sort_descending,
            theme,
        }
    }

    /// Mirrors model state into the actions (and thus every bound widget).
    pub fn sync(&self, show_hidden: bool, mode: ViewMode, sort: SortOrder, theme: &ThemeChoice) {
        set_state(&self.show_hidden, &show_hidden.to_variant());
        set_state(&self.view_mode, &mode.id().to_variant());
        set_state(&self.sort_key, &sort.key.id().to_variant());
        set_state(&self.sort_descending, &sort.descending.to_variant());
        set_state(&self.theme, &theme.id().to_variant());
    }
}

fn set_state(action: &gio::SimpleAction, value: &gtk::glib::Variant) {
    if action.state().as_ref() != Some(value) {
        action.set_state(value);
    }
}

fn action(name: &str) -> String {
    format!("{GROUP}.{name}")
}

fn radio(menu: &gio::Menu, label: &str, name: &str, target: &str) {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(&action(name)), Some(&target.to_variant()));
    menu.append_item(&item);
}

fn sort_section() -> gio::Menu {
    let keys = gio::Menu::new();
    for key in SortKey::ALL {
        radio(&keys, key.label(), "sort-key", key.id());
    }
    let order = gio::Menu::new();
    order.append(Some("Descending"), Some(&action("sort-descending")));
    let menu = gio::Menu::new();
    menu.append_section(None, &keys);
    menu.append_section(None, &order);
    menu
}

pub fn sort_menu() -> gio::Menu {
    sort_section()
}

pub fn theme_menu(registry: &ThemeRegistry) -> gio::Menu {
    let builtin = gio::Menu::new();
    for (label, choice) in [
        ("Follow System", ThemeChoice::System),
        ("Light", ThemeChoice::Light),
        ("Dark", ThemeChoice::Dark),
    ] {
        radio(&builtin, label, "theme", &choice.id());
    }
    let custom = gio::Menu::new();
    for theme in registry.customs() {
        let label = if theme.warnings.is_empty() {
            theme.theme.name.clone()
        } else {
            format!("{} ⚠", theme.theme.name)
        };
        radio(
            &custom,
            &label,
            "theme",
            &ThemeChoice::Custom(theme.id.clone()).id(),
        );
    }
    let manage = gio::Menu::new();
    manage.append(Some("Reload Themes"), Some(&action("reload-themes")));
    manage.append(
        Some("Open Themes Folder"),
        Some(&action("open-themes-folder")),
    );

    let menu = gio::Menu::new();
    menu.append_section(None, &builtin);
    if custom.n_items() > 0 {
        menu.append_section(Some("Custom"), &custom);
    }
    menu.append_section(None, &manage);
    menu
}

/// Menu for a right-click on items.
pub fn item_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Open"), Some(&action("open")));
    menu.append(Some("Copy Path"), Some(&action("copy-path")));
    menu
}

/// Menu for a right-click on empty space.
pub fn background_menu() -> gio::Menu {
    let general = gio::Menu::new();
    general.append(Some("Refresh"), Some(&action("refresh")));
    general.append(Some("Show Hidden Files"), Some(&action("show-hidden")));
    let view = gio::Menu::new();
    radio(&view, "List", "view-mode", ViewMode::List.id());
    radio(&view, "Grid", "view-mode", ViewMode::Grid.id());

    let menu = gio::Menu::new();
    menu.append_section(None, &general);
    menu.append_section(None, &view);
    menu.append_submenu(Some("Sort By"), &sort_section());
    menu
}
