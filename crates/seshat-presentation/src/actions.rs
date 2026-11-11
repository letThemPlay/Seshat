//! Window actions and the menus built on them.
//!
//! Stateful actions (hidden files, view mode, sort, theme) are the single
//! source the toolbar toggles, menus and context menus render from, so they
//! can never disagree. Command actions are enabled only where they apply.

use gtk::gio;
use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::interaction::{Action, AppAction};
use seshat_application::preferences::ThemeChoice;
use seshat_application::view::{SortKey, SortOrder, ViewMode};
use seshat_application::workspace::file_actions::{Availability, ItemKind};

use crate::app::{App, Msg, ThemeMsg};
use crate::theme::ThemeRegistry;

pub const GROUP: &str = "seshat";

/// What the command actions depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActionContext {
    /// Whether file actions apply; the application layer decides.
    pub files: Availability,
    /// Number of open branches.
    pub branches: usize,
}

/// The window action for picking a theme. Not an [`AppAction`]: only the
/// theme menu sets it.
const THEME: &str = "theme";

impl ActionContext {
    fn allows(&self, action: AppAction) -> bool {
        if let Some(file_action) = action.file_action() {
            return self.files.allows(file_action);
        }
        !action.needs_another_branch() || self.branches > 1
    }
}

#[derive(Debug, Clone)]
pub struct Actions {
    pub group: gio::SimpleActionGroup,
    commands: Vec<(AppAction, gio::SimpleAction)>,
    show_hidden: gio::SimpleAction,
    view_mode: gio::SimpleAction,
    sort_key: gio::SimpleAction,
    sort_descending: gio::SimpleAction,
    focus_mode: gio::SimpleAction,
    theme: gio::SimpleAction,
}

impl Actions {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let group = gio::SimpleActionGroup::new();

        let mut commands = Vec::new();
        for &command in AppAction::COMMANDS {
            let action = gio::SimpleAction::new(command.id(), None);
            let sender = sender.clone();
            action.connect_activate(move |_, _| sender.input_sender().emit(Msg::Perform(command)));
            group.add_action(&action);
            commands.push((command, action));
        }

        let toggle = |toggled: AppAction| {
            let action = gio::SimpleAction::new_stateful(toggled.id(), None, &false.to_variant());
            let sender = sender.clone();
            action.connect_activate(move |_, _| sender.input_sender().emit(Msg::Perform(toggled)));
            action
        };
        let show_hidden = toggle(AppAction::ToggleHidden);
        let sort_descending = toggle(AppAction::ToggleSortDescending);
        let focus_mode = toggle(Action::FocusMode.into());

        let string_action = |name: &str, initial: &str, parse: fn(&str) -> Option<Msg>| {
            let action = gio::SimpleAction::new_stateful(
                name,
                Some(&String::static_variant_type()),
                &initial.to_variant(),
            );
            let sender = sender.clone();
            let name = name.to_owned();
            action.connect_activate(move |_, param| {
                let Some(value) = param.and_then(|p| p.get::<String>()) else {
                    return;
                };
                // Ids are parsed here, so messages carry typed values.
                match parse(&value) {
                    Some(msg) => sender.input_sender().emit(msg),
                    None => tracing::warn!(action = %name, value, "unknown action value"),
                }
            });
            action
        };
        let view_mode = string_action(
            AppAction::SetViewMode(ViewMode::List).id(),
            ViewMode::List.id(),
            |id| ViewMode::from_id(id).map(|mode| Msg::Perform(AppAction::SetViewMode(mode))),
        );
        let sort_key = string_action(
            AppAction::SetSortKey(SortKey::Name).id(),
            SortKey::Name.id(),
            |id| SortKey::from_id(id).map(|key| Msg::Perform(AppAction::SetSortKey(key))),
        );
        let theme = string_action(THEME, &ThemeChoice::System.id(), |id| {
            ThemeChoice::from_id(id).map(|choice| Msg::Theme(ThemeMsg::Set(choice)))
        });

        for action in [
            &show_hidden,
            &sort_descending,
            &focus_mode,
            &view_mode,
            &sort_key,
            &theme,
        ] {
            group.add_action(action);
        }

        Self {
            group,
            commands,
            show_hidden,
            view_mode,
            sort_key,
            sort_descending,
            focus_mode,
            theme,
        }
    }

    /// Mirrors model state into the actions (and thus every bound widget).
    pub fn sync(
        &self,
        show_hidden: bool,
        mode: ViewMode,
        sort: SortOrder,
        theme: &ThemeChoice,
        focus_mode: bool,
        context: ActionContext,
    ) {
        set_state(&self.focus_mode, &focus_mode.to_variant());
        set_state(&self.show_hidden, &show_hidden.to_variant());
        set_state(&self.view_mode, &mode.id().to_variant());
        set_state(&self.sort_key, &sort.key.id().to_variant());
        set_state(&self.sort_descending, &sort.descending.to_variant());
        set_state(&self.theme, &theme.id().to_variant());
        for (command, action) in &self.commands {
            let enabled = context.allows(*command);
            if action.is_enabled() != enabled {
                action.set_enabled(enabled);
            }
        }
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

/// The detailed name widgets use to activate `command` (`seshat.refresh`).
pub fn detailed_name(command: AppAction) -> String {
    action(command.id())
}

/// The value a stateful action with a parameter is set to for `command`
/// (the view mode or sort key it picks), if it has one.
pub fn target(command: AppAction) -> Option<&'static str> {
    match command {
        AppAction::SetViewMode(mode) => Some(mode.id()),
        AppAction::SetSortKey(key) => Some(key.id()),
        _ => None,
    }
}

fn item(menu: &gio::Menu, label: &str, command: AppAction, accel: Option<&str>) {
    let entry = gio::MenuItem::new(Some(label), Some(&action(command.id())));
    if let Some(accel) = accel {
        entry.set_attribute_value("accel", Some(&accel.to_variant()));
    }
    menu.append_item(&entry);
}

fn radio(menu: &gio::Menu, label: &str, name: &str, target: &str) {
    let entry = gio::MenuItem::new(Some(label), None);
    entry.set_action_and_target_value(Some(&action(name)), Some(&target.to_variant()));
    menu.append_item(&entry);
}

pub fn sort_menu() -> gio::Menu {
    let keys = gio::Menu::new();
    for key in SortKey::ALL {
        radio(
            &keys,
            key.label(),
            AppAction::SetSortKey(key).id(),
            key.id(),
        );
    }
    let order = gio::Menu::new();
    item(&order, "Descending", AppAction::ToggleSortDescending, None);
    let menu = gio::Menu::new();
    menu.append_section(None, &keys);
    menu.append_section(None, &order);
    menu
}

pub fn theme_menu(registry: &ThemeRegistry) -> gio::Menu {
    let builtin = gio::Menu::new();
    for (label, choice) in [
        ("Follow System", ThemeChoice::System),
        ("Light", ThemeChoice::Light),
        ("Dark", ThemeChoice::Dark),
    ] {
        radio(&builtin, label, THEME, &choice.id());
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
            THEME,
            &ThemeChoice::Custom(theme.id.clone()).id(),
        );
    }
    let manage = gio::Menu::new();
    item(&manage, "Reload Themes", AppAction::ReloadThemes, None);
    item(
        &manage,
        "Open Themes Folder",
        AppAction::OpenThemesFolder,
        None,
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
pub fn item_menu(in_trash: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if in_trash {
        let main = gio::Menu::new();
        item(&main, "Restore", AppAction::Restore, None);
        item(&main, "Copy Path", AppAction::CopyPaths, None);
        let danger = gio::Menu::new();
        item(
            &danger,
            "Delete Permanently…",
            AppAction::Delete,
            Some("<Shift>Delete"),
        );
        menu.append_section(None, &main);
        menu.append_section(None, &danger);
        return menu;
    }
    let open = gio::Menu::new();
    item(&open, "Open", AppAction::OpenSelection, Some("Return"));
    let edit = gio::Menu::new();
    item(&edit, "Cut", Action::Cut.into(), Some("<Control>x"));
    item(&edit, "Copy", Action::Yank.into(), Some("<Control>c"));
    item(
        &edit,
        "Copy Path",
        AppAction::CopyPaths,
        Some("<Control><Shift>c"),
    );
    item(&edit, "Rename…", AppAction::Rename, Some("F2"));
    let danger = gio::Menu::new();
    item(&danger, "Move to Trash", AppAction::Trash, Some("Delete"));
    item(
        &danger,
        "Delete Permanently…",
        AppAction::Delete,
        Some("<Shift>Delete"),
    );
    menu.append_section(None, &open);
    menu.append_section(None, &edit);
    menu.append_section(None, &danger);
    menu
}

/// Menu for a right-click on empty space.
pub fn background_menu(in_trash: bool) -> gio::Menu {
    let general = gio::Menu::new();
    if in_trash {
        item(&general, "Empty Trash…", AppAction::EmptyTrash, None);
    } else {
        item(
            &general,
            "New Folder…",
            AppAction::NewItem(ItemKind::Folder),
            Some("<Control><Shift>n"),
        );
        item(
            &general,
            "New File…",
            AppAction::NewItem(ItemKind::File),
            None,
        );
        item(&general, "Paste", Action::Paste.into(), Some("<Control>v"));
    }
    let view = gio::Menu::new();
    item(&view, "Refresh", AppAction::Refresh, Some("F5"));
    item(
        &view,
        "Show Hidden Files",
        AppAction::ToggleHidden,
        Some("<Control>h"),
    );
    radio(
        &view,
        "List",
        AppAction::SetViewMode(ViewMode::List).id(),
        ViewMode::List.id(),
    );
    radio(
        &view,
        "Grid",
        AppAction::SetViewMode(ViewMode::Grid).id(),
        ViewMode::Grid.id(),
    );

    let workspace = gio::Menu::new();
    item(&workspace, "Focus Mode", Action::FocusMode.into(), None);
    item(&workspace, "Branches…", AppAction::ShowBranches, None);
    item(&workspace, "Close Branch", AppAction::CloseBranch, None);
    item(&workspace, "Keys and Commands", AppAction::Help, None);

    let menu = gio::Menu::new();
    menu.append_section(None, &general);
    menu.append_section(None, &view);
    menu.append_submenu(Some("Sort By"), &sort_menu());
    menu.append_section(None, &workspace);
    menu
}

#[cfg(test)]
mod tests {
    use super::ActionContext;
    use seshat_application::interaction::{Action, AppAction};
    use seshat_application::workspace::file_actions::Availability;

    #[test]
    fn file_commands_follow_the_application_rules() {
        let folder_one = ActionContext {
            files: Availability {
                selected: 1,
                in_trash: false,
                in_folder: true,
            },
            branches: 1,
        };
        let trash_one = ActionContext {
            files: Availability {
                selected: 1,
                in_trash: true,
                in_folder: false,
            },
            ..folder_one
        };
        assert!(folder_one.allows(AppAction::Rename));
        assert!(!folder_one.allows(AppAction::Restore));
        assert!(trash_one.allows(AppAction::Restore));
        assert!(!trash_one.allows(AppAction::Trash), "already in the Trash");
        assert!(!trash_one.allows(Action::Paste.into()));
    }

    #[test]
    fn branch_commands_need_another_branch() {
        let one = ActionContext::default();
        assert!(!one.allows(AppAction::CloseBranch), "the last branch stays");
        assert!(!one.allows(Action::BranchNext.into()));
        let two = ActionContext { branches: 2, ..one };
        assert!(two.allows(AppAction::CloseBranch));
        assert!(two.allows(AppAction::Help));
    }
}
