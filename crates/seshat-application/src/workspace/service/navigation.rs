//! Navigation through the workspace: moving between panels and branches,
//! opening folders (context-aware), the jump list, closing and pruning
//! branches, workspace undo, and Focus Mode.

use seshat_domain::Direction;
use seshat_domain::entities::{FileEntry, Location};
use seshat_domain::workspace::{BranchId, Opened, PanelId, PruneMotion, Workspace, WorkspaceError};

use super::{Effect, WorkspaceService};
use crate::workspace::undo::{PruneError, PruneOutcome, UndoError, UndoOutcome, WorkspaceChange};

impl WorkspaceService {
    pub fn activate_panel(&mut self, panel: PanelId) -> Result<bool, WorkspaceError> {
        self.try_navigate(|w| w.activate_panel(panel))
    }

    pub fn move_left(&mut self) -> bool {
        self.navigate(Workspace::move_left)
    }

    pub fn move_right(&mut self) -> bool {
        self.navigate(Workspace::move_right)
    }

    /// Backspace / "parent folder": the parent panel, or for a root panel
    /// a new parent panel above it.
    pub fn go_parent(&mut self) -> bool {
        self.navigate(|w| {
            if w.move_left() {
                return true;
            }
            let root = w.active_panel();
            w.insert_parent(root).is_ok()
        })
    }

    pub fn go_back(&mut self) -> bool {
        self.navigate(Workspace::go_back)
    }

    pub fn go_forward(&mut self) -> bool {
        self.navigate(Workspace::go_forward)
    }

    pub fn cycle_branch(&mut self, direction: Direction) -> bool {
        self.navigate(|w| w.cycle_branch(direction))
    }

    pub fn activate_branch(&mut self, branch: BranchId) -> Result<bool, WorkspaceError> {
        self.try_navigate(|w| w.activate_branch(branch))
    }

    /// Closes a branch; listings of removed panels are dropped. Never
    /// touches files, and running operations are unaffected (they hold
    /// their own paths).
    pub fn close_branch(&mut self, branch: BranchId) -> Result<Vec<PanelId>, WorkspaceError> {
        let removed = self.workspace.close_branch(branch)?;
        self.forget_panels(&removed);
        self.after_navigation();
        Ok(removed)
    }

    /// Backward pruning (`db`, `dB`): removes panels of `branch` from
    /// `panel` back to the destination `motion` resolves to, which becomes
    /// active with its state intact. See [`Workspace::prune`].
    ///
    /// Workspace structure only: no file is ever touched, and the only
    /// effects are unwatching the removed panels and saving. Refused in
    /// Focus Mode (decision B9). A completed prune can be undone with
    /// [`Self::undo_workspace`].
    pub fn prune_back(
        &mut self,
        branch: BranchId,
        panel: PanelId,
        motion: PruneMotion,
    ) -> Result<PruneOutcome, PruneError> {
        if self.workspace.focus_mode() {
            return Err(PruneError::FocusMode);
        }
        let pruned = self.workspace.prune(branch, panel, motion)?;
        let removed = pruned.removed();
        self.forget_panels(&removed);
        let outcome = PruneOutcome {
            destination: pruned.destination(),
            removed: removed.len(),
            deselected: pruned.deselected(),
            ended_branch: pruned.ends_branch(),
        };
        tracing::debug!(
            ?motion,
            branch = %branch,
            destination = %outcome.destination,
            removed = outcome.removed,
            ended_branch = outcome.ended_branch,
            "workspace pruned"
        );
        self.undo.push(WorkspaceChange::Prune(pruned));
        self.after_navigation();
        Ok(outcome)
    }

    /// Undoes the most recent workspace change (a prune), putting its
    /// panels back with their state. Never undoes file operations. Refused
    /// in Focus Mode (decision B9). A change that can no longer be undone
    /// is dropped from the history, so the next undo reaches the one
    /// before it (decision B5).
    pub fn undo_workspace(&mut self) -> Result<UndoOutcome, UndoError> {
        if self.workspace.focus_mode() {
            return Err(UndoError::FocusMode);
        }
        let change = self.undo.pop().ok_or(UndoError::NothingToUndo)?;
        let outcome = match change {
            WorkspaceChange::Prune(pruned) => {
                self.workspace.restore_pruned(&pruned)?;
                UndoOutcome {
                    restored: pruned.removed().len(),
                }
            }
        };
        self.after_navigation();
        Ok(outcome)
    }

    /// How many workspace changes can be undone.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Opens `entry` from `panel`. This is the single entry point for
    /// opening: `l`, `Enter`, double-click and the command line all come
    /// here, so they resolve navigation identically.
    ///
    /// Folders are resolved by the workspace (context-aware navigation):
    /// continue the active branch, reactivate an existing branch, or create
    /// a new one; see [`Workspace::open_directory`]. Files are launched.
    /// Returns how a folder was resolved, or `None` for a file.
    ///
    /// [`Workspace::open_directory`]: seshat_domain::workspace::Workspace::open_directory
    pub fn open_entry(
        &mut self,
        panel: PanelId,
        entry: &FileEntry,
    ) -> Result<Option<Opened>, WorkspaceError> {
        if !entry.is_browsable() {
            self.effects
                .push(Effect::Launch(vec![entry.location.clone()]));
            return Ok(None);
        }
        self.workspace.activate_panel(panel)?;
        let opened =
            self.workspace
                .open_directory(panel, entry.location.clone(), entry.identity, None)?;
        if let Some(state) = self.workspace.state_mut(panel) {
            state.cursor = Some(entry.location.clone());
        }
        // Renamed or replaced folders: their listings and watches belong to
        // the old folder and are read again.
        self.forget_panels(&opened.refreshed);
        tracing::debug!(
            resolution = ?opened.resolution,
            branch = %opened.branch,
            panel = %opened.active,
            created = opened.created,
            refreshed = opened.refreshed.len(),
            "navigation resolved"
        );
        self.after_navigation();
        Ok(Some(opened))
    }

    /// Opens a location from outside the tree (sidebar, address bar,
    /// devices): the active branch's panel for it, or a new root branch.
    pub fn open_location(&mut self, location: Location) {
        self.workspace.open_root(location, self.defaults.inherit());
        self.after_navigation();
    }

    /// Breadcrumb navigation: activates the active branch's panel for
    /// `location`; for an ancestor of the root, inserts parent panels up to
    /// it; anything else opens a new root branch.
    pub fn open_ancestor(&mut self, location: Location) {
        if let Some(panel) = self.workspace.find_on_active_path(&location) {
            let _ = self.workspace.activate_panel(panel);
        } else if let Location::Path(target) = &location
            && let Some(&root) = self.workspace.active_path().first()
            && matches!(self.location(root), Some(Location::Path(r)) if r.starts_with(target))
        {
            let mut top = root;
            while self.location(top) != Some(&location) {
                match self.workspace.insert_parent(top) {
                    Ok(parent) => top = parent,
                    Err(_) => break,
                }
            }
        } else {
            self.workspace.open_root(location, self.defaults.inherit());
        }
        self.after_navigation();
    }

    pub fn toggle_focus_mode(&mut self) -> bool {
        let on = !self.workspace.focus_mode();
        self.workspace.set_focus_mode(on);
        self.effects.push(Effect::Persist);
        on
    }

    pub fn set_focus_mode(&mut self, on: bool) {
        if self.workspace.set_focus_mode(on) {
            self.effects.push(Effect::Persist);
        }
    }

    pub fn set_retain_focus_on_branch_switch(&mut self, retain: bool) {
        self.workspace.set_retain_focus_on_branch_switch(retain);
    }
}
