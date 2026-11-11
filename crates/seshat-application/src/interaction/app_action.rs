//! Everything the window can be asked to do, whatever asked for it: a key,
//! a menu item, a toolbar button, a shortcut, a mouse button or the command
//! line. Every entry point names an [`AppAction`] and the window carries it
//! out in one place, so the same request can never behave differently
//! depending on where it came from.

use seshat_domain::view::{SortKey, ViewMode};

use super::Action;
use crate::workspace::file_actions::{FileAction, ItemKind};

/// A request to the window. [`AppAction::Key`] wraps the key-bindable
/// [`Action`]s, which users can rebind in the settings file; the other
/// variants are reached through menus, shortcuts and commands only, so
/// adding one never changes what can be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    /// A key-bindable action, the same from keys, menus and commands.
    Key(Action),
    /// Open the targets (the selection, or the cursor item).
    OpenSelection,
    /// Put the targets' paths on the clipboard.
    CopyPaths,
    /// Read the active panel again.
    Refresh,
    Rename,
    /// Move the targets to the Trash.
    Trash,
    /// Delete the targets permanently (always confirmed).
    Delete,
    /// Restore the targets from the Trash.
    Restore,
    NewItem(ItemKind),
    EmptyTrash,
    ToggleHidden,
    SetViewMode(ViewMode),
    SetSortKey(SortKey),
    ToggleSortDescending,
    /// Show the branch overview.
    ShowBranches,
    /// Close the active branch.
    CloseBranch,
    /// Show keys and commands.
    Help,
    ReloadThemes,
    OpenThemesFolder,
    /// Type a location into the address bar.
    EditAddress,
    /// Open the context menu from the keyboard (Menu key, Shift+F10).
    KeyboardContextMenu,
}

impl From<Action> for AppAction {
    fn from(action: Action) -> Self {
        Self::Key(action)
    }
}

impl AppAction {
    /// The window actions without state that menus and shortcuts refer
    /// to by [`AppAction::id`].
    pub const COMMANDS: &[AppAction] = &[
        Self::OpenSelection,
        Self::CopyPaths,
        Self::Refresh,
        Self::ReloadThemes,
        Self::OpenThemesFolder,
        Self::Key(Action::Cut),
        Self::Key(Action::Yank),
        Self::Key(Action::Paste),
        Self::Rename,
        Self::Trash,
        Self::Delete,
        Self::Restore,
        Self::NewItem(ItemKind::Folder),
        Self::NewItem(ItemKind::File),
        Self::EmptyTrash,
        Self::ShowBranches,
        Self::CloseBranch,
        Self::Key(Action::BranchNext),
        Self::Key(Action::BranchPrevious),
        Self::Help,
    ];

    /// The name of the window action that carries this out. This is the
    /// only place those names are written. For actions with a value
    /// (view mode, sort key) it names the one stateful action shared by
    /// all values.
    pub fn id(self) -> &'static str {
        match self {
            Self::Key(action) => action.id(),
            Self::OpenSelection => "open-selection",
            Self::CopyPaths => "copy-path",
            Self::Refresh => "refresh",
            Self::Rename => "rename",
            Self::Trash => "trash",
            Self::Delete => "delete",
            Self::Restore => "restore",
            Self::NewItem(ItemKind::Folder) => "new-folder",
            Self::NewItem(ItemKind::File) => "new-file",
            Self::EmptyTrash => "empty-trash",
            Self::ToggleHidden => "show-hidden",
            Self::SetViewMode(_) => "view-mode",
            Self::SetSortKey(_) => "sort-key",
            Self::ToggleSortDescending => "sort-descending",
            Self::ShowBranches => "branches",
            Self::CloseBranch => "close-branch",
            Self::Help => "help",
            Self::ReloadThemes => "reload-themes",
            Self::OpenThemesFolder => "open-themes-folder",
            Self::EditAddress => "edit-address",
            Self::KeyboardContextMenu => "context-menu",
        }
    }

    /// The file action this stands for, whose availability the
    /// application layer decides ([`Availability`]).
    ///
    /// [`Availability`]: crate::workspace::file_actions::Availability
    pub fn file_action(self) -> Option<FileAction> {
        Some(match self {
            Self::OpenSelection => FileAction::Open,
            Self::CopyPaths => FileAction::CopyPath,
            Self::Key(Action::Cut) => FileAction::Cut,
            Self::Key(Action::Yank) => FileAction::Copy,
            Self::Key(Action::Paste) => FileAction::Paste,
            Self::Rename => FileAction::Rename,
            Self::Trash => FileAction::Trash,
            Self::Delete => FileAction::Delete,
            Self::Restore => FileAction::Restore,
            Self::NewItem(ItemKind::Folder) => FileAction::NewFolder,
            Self::NewItem(ItemKind::File) => FileAction::NewFile,
            Self::EmptyTrash => FileAction::EmptyTrash,
            _ => return None,
        })
    }

    /// Whether it only makes sense with more than one branch open.
    pub fn needs_another_branch(self) -> bool {
        matches!(
            self,
            Self::CloseBranch | Self::Key(Action::BranchNext | Action::BranchPrevious)
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn window_action_names_are_unique() {
        let mut seen = HashSet::new();
        for action in AppAction::COMMANDS {
            assert!(seen.insert(action.id()), "{} twice", action.id());
        }
        // The stateful actions are created separately; none may share a
        // name with a command.
        for stateful in [
            AppAction::ToggleHidden,
            AppAction::SetViewMode(ViewMode::List),
            AppAction::SetSortKey(SortKey::Name),
            AppAction::ToggleSortDescending,
            AppAction::Key(Action::FocusMode),
        ] {
            assert!(seen.insert(stateful.id()), "{}", stateful.id());
        }
    }

    #[test]
    fn every_file_action_is_offered_as_a_command() {
        let offered: Vec<FileAction> = AppAction::COMMANDS
            .iter()
            .filter_map(|a| a.file_action())
            .collect();
        for action in [
            FileAction::Open,
            FileAction::CopyPath,
            FileAction::Cut,
            FileAction::Copy,
            FileAction::Paste,
            FileAction::Rename,
            FileAction::Trash,
            FileAction::Delete,
            FileAction::Restore,
            FileAction::NewFolder,
            FileAction::NewFile,
            FileAction::EmptyTrash,
        ] {
            assert!(offered.contains(&action), "{action:?}");
        }
    }

    #[test]
    fn branch_commands_need_another_branch() {
        assert!(AppAction::CloseBranch.needs_another_branch());
        assert!(AppAction::from(Action::BranchNext).needs_another_branch());
        assert!(!AppAction::ShowBranches.needs_another_branch());
        assert!(!AppAction::Refresh.needs_another_branch());
    }
}
