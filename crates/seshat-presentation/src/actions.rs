//! Window actions and the menus built on them.
//!
//! Stateful actions (hidden files, view mode, sort, theme) are the single
//! source the toolbar toggles, menus and context menus render from, so they
//! can never disagree. Command actions are enabled only where they apply.

use std::collections::HashMap;

use gtk::gio;
use gtk::prelude::*;
use relm4::ComponentSender;
use seshat_application::interaction::Action;
use seshat_application::preferences::ThemeChoice;
use seshat_application::view::{SortKey, SortOrder, ViewMode};
use seshat_application::workspace::TransferKind;
use seshat_application::workspace::file_actions::{Availability, FileAction, ItemKind};

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

/// An action name and the message it sends.
type Command = (&'static str, fn() -> Msg);

const COMMANDS: &[Command] = &[
    ("open", || Msg::OpenSelection),
    ("copy-path", || Msg::CopyPaths),
    ("refresh", || Msg::Refresh),
    ("reload-themes", || Msg::Theme(ThemeMsg::Reload)),
    ("open-themes-folder", || Msg::Theme(ThemeMsg::OpenFolder)),
    ("cut", || Msg::ClipboardStore(TransferKind::Move)),
    ("copy", || Msg::ClipboardStore(TransferKind::Copy)),
    ("paste", || Msg::Paste),
    ("rename", || Msg::RenameSelection),
    ("trash", || Msg::TrashSelection),
    ("delete", || Msg::DeleteSelection),
    ("restore", || Msg::RestoreSelection),
    ("new-folder", || Msg::NewItem(ItemKind::Folder)),
    ("new-file", || Msg::NewItem(ItemKind::File)),
    ("empty-trash", || Msg::EmptyTrash),
    ("branches", || Msg::ShowBranches),
    ("close-branch", || Msg::CloseBranch),
    ("next-branch", || Msg::Perform(Action::BranchNext)),
    ("previous-branch", || Msg::Perform(Action::BranchPrevious)),
    ("help", || Msg::ShowHelp),
];

/// The file action a command action stands for, if any.
fn file_action(command: &str) -> Option<FileAction> {
    Some(match command {
        "open" => FileAction::Open,
        "copy-path" => FileAction::CopyPath,
        "cut" => FileAction::Cut,
        "copy" => FileAction::Copy,
        "paste" => FileAction::Paste,
        "rename" => FileAction::Rename,
        "trash" => FileAction::Trash,
        "delete" => FileAction::Delete,
        "restore" => FileAction::Restore,
        "new-folder" => FileAction::NewFolder,
        "new-file" => FileAction::NewFile,
        "empty-trash" => FileAction::EmptyTrash,
        _ => return None,
    })
}

impl ActionContext {
    fn allows(&self, command: &str) -> bool {
        if let Some(action) = file_action(command) {
            return self.files.allows(action);
        }
        match command {
            "close-branch" | "next-branch" | "previous-branch" => self.branches > 1,
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Actions {
    pub group: gio::SimpleActionGroup,
    commands: HashMap<&'static str, gio::SimpleAction>,
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

        let mut commands = HashMap::new();
        for &(name, msg) in COMMANDS {
            let action = gio::SimpleAction::new(name, None);
            let sender = sender.clone();
            action.connect_activate(move |_, _| sender.input_sender().emit(msg()));
            group.add_action(&action);
            commands.insert(name, action);
        }

        let toggle = |name: &str, msg: fn() -> Msg| {
            let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            let sender = sender.clone();
            action.connect_activate(move |_, _| sender.input_sender().emit(msg()));
            action
        };
        let show_hidden = toggle("show-hidden", || Msg::ToggleHidden);
        let sort_descending = toggle("sort-descending", || Msg::ToggleSortDescending);
        let focus_mode = toggle("focus-mode", || Msg::Perform(Action::FocusMode));

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
        let view_mode = string_action("view-mode", ViewMode::List.id(), |id| {
            ViewMode::from_id(id).map(Msg::SetViewMode)
        });
        let sort_key = string_action("sort-key", SortKey::Name.id(), |id| {
            SortKey::from_id(id).map(Msg::SetSortKey)
        });
        let theme = string_action("theme", &ThemeChoice::System.id(), |id| {
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
        for (name, action) in &self.commands {
            let enabled = context.allows(name);
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

fn item(menu: &gio::Menu, label: &str, name: &str, accel: Option<&str>) {
    let entry = gio::MenuItem::new(Some(label), Some(&action(name)));
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
        radio(&keys, key.label(), "sort-key", key.id());
    }
    let order = gio::Menu::new();
    item(&order, "Descending", "sort-descending", None);
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
    item(&manage, "Reload Themes", "reload-themes", None);
    item(&manage, "Open Themes Folder", "open-themes-folder", None);

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
        item(&main, "Restore", "restore", None);
        item(&main, "Copy Path", "copy-path", None);
        let danger = gio::Menu::new();
        item(
            &danger,
            "Delete Permanently…",
            "delete",
            Some("<Shift>Delete"),
        );
        menu.append_section(None, &main);
        menu.append_section(None, &danger);
        return menu;
    }
    let open = gio::Menu::new();
    item(&open, "Open", "open", Some("Return"));
    let edit = gio::Menu::new();
    item(&edit, "Cut", "cut", Some("<Control>x"));
    item(&edit, "Copy", "copy", Some("<Control>c"));
    item(&edit, "Copy Path", "copy-path", Some("<Control><Shift>c"));
    item(&edit, "Rename…", "rename", Some("F2"));
    let danger = gio::Menu::new();
    item(&danger, "Move to Trash", "trash", Some("Delete"));
    item(
        &danger,
        "Delete Permanently…",
        "delete",
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
        item(&general, "Empty Trash…", "empty-trash", None);
    } else {
        item(
            &general,
            "New Folder…",
            "new-folder",
            Some("<Control><Shift>n"),
        );
        item(&general, "New File…", "new-file", None);
        item(&general, "Paste", "paste", Some("<Control>v"));
    }
    let view = gio::Menu::new();
    item(&view, "Refresh", "refresh", Some("F5"));
    item(
        &view,
        "Show Hidden Files",
        "show-hidden",
        Some("<Control>h"),
    );
    radio(&view, "List", "view-mode", ViewMode::List.id());
    radio(&view, "Grid", "view-mode", ViewMode::Grid.id());

    let workspace = gio::Menu::new();
    item(&workspace, "Focus Mode", "focus-mode", None);
    item(&workspace, "Branches…", "branches", None);
    item(&workspace, "Close Branch", "close-branch", None);
    item(&workspace, "Keys and Commands", "help", None);

    let menu = gio::Menu::new();
    menu.append_section(None, &general);
    menu.append_section(None, &view);
    menu.append_submenu(Some("Sort By"), &sort_menu());
    menu.append_section(None, &workspace);
    menu
}

#[cfg(test)]
mod tests {
    use super::{ActionContext, COMMANDS, file_action};
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
        assert!(folder_one.allows("rename"));
        assert!(!folder_one.allows("restore"));
        assert!(trash_one.allows("restore"));
        assert!(!trash_one.allows("trash"), "already in the Trash");
        assert!(!trash_one.allows("paste"));
        // Every file command name is mapped (a typo would leave it always on).
        for name in [
            "open",
            "copy-path",
            "cut",
            "copy",
            "paste",
            "rename",
            "trash",
            "delete",
            "restore",
            "new-folder",
            "new-file",
            "empty-trash",
        ] {
            assert!(file_action(name).is_some(), "{name}");
            assert!(COMMANDS.iter().any(|(n, _)| *n == name), "{name}");
        }
    }

    #[test]
    fn branch_commands_need_another_branch() {
        let one = ActionContext::default();
        assert!(!one.allows("close-branch"), "the last branch stays");
        assert!(!one.allows("next-branch"));
        let two = ActionContext { branches: 2, ..one };
        assert!(two.allows("close-branch"));
        assert!(two.allows("help"));
    }
}
