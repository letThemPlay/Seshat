//! Opening folders, the heart of context-aware navigation (spec 2, ADR
//! 0006): continue the active branch, reactivate an existing branch or
//! start a new one; new roots and parent panels; and following folders
//! that were renamed or replaced.

use super::{Workspace, WorkspaceError};
use crate::entities::{DirectoryIdentity, Location};
use crate::workspace::{BranchId, PanelId, PanelState, Unavailability};

/// Result of opening a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The panel of the opened folder.
    pub panel: PanelId,
    /// Where the user lands: the opened folder's panel, or, when an
    /// existing branch is reactivated, that branch's remembered panel if
    /// it lies at or below the opened folder (decision C1).
    pub active: PanelId,
    /// The branch that is now active.
    pub branch: BranchId,
    pub resolution: Resolution,
    /// A new panel was created (otherwise an existing one was activated).
    pub created: bool,
    /// A new branch was created for it.
    pub new_branch: bool,
    /// Panels whose folder changed under them: renamed (their location was
    /// updated) or replaced by a different folder (their state was reset).
    /// Their listings must be read again.
    pub refreshed: Vec<PanelId>,
}

impl Opened {
    /// An existing panel was activated; nothing was created.
    fn existing(
        panel: PanelId,
        active: PanelId,
        branch: BranchId,
        resolution: Resolution,
        refreshed: Vec<PanelId>,
    ) -> Self {
        Self {
            panel,
            active,
            branch,
            resolution,
            created: false,
            new_branch: false,
            refreshed,
        }
    }

    /// A new panel was created and became active, extending its branch or,
    /// with `new_branch`, starting one.
    fn created(
        panel: PanelId,
        branch: BranchId,
        new_branch: bool,
        refreshed: Vec<PanelId>,
    ) -> Self {
        Self {
            panel,
            active: panel,
            branch,
            resolution: if new_branch {
                Resolution::CreateNewBranch
            } else {
                Resolution::ContinueActiveBranch
            },
            created: true,
            new_branch,
            refreshed,
        }
    }
}

/// How opening a folder was resolved (context-aware navigation, §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// The folder is, or becomes, the next step of the active branch.
    ContinueActiveBranch,
    /// The folder already belonged to another branch, which comes back with
    /// its remembered panels and state.
    ReactivateExistingBranch,
    /// The folder starts a new branch from the current panel.
    CreateNewBranch,
}

impl Workspace {
    /// Opens `location` as a child of `from`.
    ///
    /// An existing child showing it is activated instead of duplicated.
    /// Opening from a branch tip extends that branch; opening from a panel
    /// that already has children starts a new branch, so no existing path
    /// is ever replaced. The new panel inherits `from`'s presentation
    /// settings unless `state` is given.
    pub fn open_child(
        &mut self,
        from: PanelId,
        location: Location,
        state: Option<PanelState>,
    ) -> Result<Opened, WorkspaceError> {
        self.open_directory(from, location, None, state)
    }

    /// Opens the folder at `location` (with filesystem `identity`, when
    /// known) from panel `from`, resolving it in this order (§4):
    ///
    /// 1. **Continue the active branch**: `from` already has a child for
    ///    the folder on the active branch (it is activated, its downstream
    ///    panels kept), or `from` is the branch's tip (the branch grows).
    /// 2. **Reactivate an existing branch**: `from` has a child for the
    ///    folder on another branch. The most recently active branch through
    ///    it comes back, landing on its remembered panel when that lies at
    ///    or below the folder (C1).
    /// 3. **Create a new branch** from `from`.
    ///
    /// Children are matched by location. With identities (C3): a child at
    /// the same location whose identity changed is a different folder now,
    /// so its state is reset and its descendants are marked unavailable; a
    /// child with the same identity under another name was renamed, so it
    /// (and everything below it) follows the new name. Nothing is ever
    /// duplicated, and only children of `from` are considered (C4).
    pub fn open_directory(
        &mut self,
        from: PanelId,
        location: Location,
        identity: Option<DirectoryIdentity>,
        state: Option<PanelState>,
    ) -> Result<Opened, WorkspaceError> {
        let parent = self
            .panels
            .get(&from)
            .ok_or(WorkspaceError::UnknownPanel(from))?;
        let children = parent.children.clone();
        let inherited = parent.state.inherit();
        let by_location = children
            .iter()
            .copied()
            .find(|c| self.panels[c].location == location);
        let by_identity = identity.and_then(|id| {
            children
                .iter()
                .copied()
                .find(|c| self.panels[c].identity == Some(id))
        });

        let mut refreshed = Vec::new();
        let existing = match (by_location, by_identity) {
            (Some(child), _) => {
                let known = self.panels[&child].identity;
                if let (Some(new), Some(old)) = (identity, known)
                    && new != old
                {
                    // Same name, different folder: never reuse its state.
                    refreshed = self.reset_replaced(child, new);
                } else if identity.is_some() {
                    self.panels.get_mut(&child).expect("child").identity = identity;
                }
                Some(child)
            }
            (None, Some(child)) => {
                // Same folder under a new name: follow the rename.
                refreshed = self.relocate(child, &location);
                Some(child)
            }
            (None, None) => None,
        };

        if let Some(child) = existing {
            let (branch, active, resolution) = if self.active_path().contains(&child) {
                (self.active, child, Resolution::ContinueActiveBranch)
            } else {
                let branch = *self
                    .branches_through(child)
                    .first()
                    .expect("every panel lies on a branch");
                let cursor = self.branches[&branch].cursor;
                let land = if self.ancestors_or_self(cursor).contains(&child) {
                    cursor
                } else {
                    child
                };
                (branch, land, Resolution::ReactivateExistingBranch)
            };
            self.set_active(branch, active, true);
            return Ok(Opened::existing(
                child, active, branch, resolution, refreshed,
            ));
        }

        let state = state.unwrap_or(inherited);
        let extends = self.branch_with_tip(from).map(|b| b.id);
        let child = self.add_panel(location, Some(from), state);
        self.panels.get_mut(&child).expect("just added").identity = identity;
        self.panels
            .get_mut(&from)
            .expect("checked above")
            .children
            .push(child);
        let (branch, new_branch) = match extends {
            Some(branch) => {
                self.branches.get_mut(&branch).expect("tip's branch").tip = child;
                (branch, false)
            }
            None => (self.add_branch(child), true),
        };
        self.set_active(branch, child, true);
        Ok(Opened::created(child, branch, new_branch, refreshed))
    }

    /// Opens a folder several levels below panel `from`, one folder at a
    /// time: `steps` lists each folder on the way, the last being the one
    /// wanted (decision F5, used by Fuzzy Find). Each step follows
    /// [`Self::open_directory`], so existing panels are reused (continuing
    /// the active branch or reactivating another) and a new branch is
    /// started only where continuing would replace a path. Each parent's
    /// cursor is left on the folder opened below it, as if the user had
    /// pressed `l` at every level.
    ///
    /// The whole walk is one step of the jump list (D2): Back returns to
    /// the panel that was active before. With no `steps`, `from` itself
    /// is activated. Returns how the last step was resolved, with the
    /// `refreshed` panels of every step.
    pub fn open_path(
        &mut self,
        from: PanelId,
        steps: Vec<Location>,
    ) -> Result<Opened, WorkspaceError> {
        if !self.panels.contains_key(&from) {
            return Err(WorkspaceError::UnknownPanel(from));
        }
        let before = self.active_panel();
        let jumps = self.jumps.clone();
        let mut parent = from;
        let mut opened = None;
        let mut refreshed = Vec::new();
        for step in steps {
            if let Some(state) = self.state_mut(parent) {
                state.cursor = Some(step.clone());
            }
            let next = self.open_directory(parent, step, None, None)?;
            refreshed.extend(next.refreshed.iter().copied());
            parent = next.panel;
            opened = Some(next);
        }
        let mut opened = match opened {
            Some(opened) => opened,
            None => {
                self.activate_panel(from)?;
                let branch = self.active;
                Opened::existing(
                    from,
                    from,
                    branch,
                    Resolution::ContinueActiveBranch,
                    Vec::new(),
                )
            }
        };
        // One jump for the whole walk, not one per folder.
        self.jumps = jumps;
        if self.active_panel() != before {
            self.jumps.record(before);
        }
        refreshed.sort();
        refreshed.dedup();
        opened.refreshed = refreshed;
        Ok(opened)
    }

    /// Opens `location` from outside the tree (sidebar, address bar,
    /// devices). In order: the active branch's panel for it; an existing
    /// root panel for it, whose branch comes back where it was left
    /// (decision C5); otherwise a new root with its own branch.
    pub fn open_root(&mut self, location: Location, state: PanelState) -> Opened {
        if let Some(existing) = self.find_on_active_path(&location) {
            self.set_active(self.active, existing, true);
            return Opened::existing(
                existing,
                existing,
                self.active,
                Resolution::ContinueActiveBranch,
                Vec::new(),
            );
        }
        let root = self
            .roots
            .iter()
            .copied()
            .filter(|r| self.panels[r].location == location)
            .max_by_key(|&r| {
                self.branches_through(r)
                    .first()
                    .map_or(0, |b| self.branches[b].last_active)
            });
        if let Some(root) = root {
            let branch = *self
                .branches_through(root)
                .first()
                .expect("every panel lies on a branch");
            // A root is above every panel of its branch, so the remembered
            // panel is always at or below it.
            let active = self.branches[&branch].cursor;
            self.set_active(branch, active, true);
            return Opened::existing(
                root,
                active,
                branch,
                Resolution::ReactivateExistingBranch,
                Vec::new(),
            );
        }
        let panel = self.add_panel(location, None, state);
        self.roots.push(panel);
        let branch = self.add_branch(panel);
        self.set_active(branch, panel, true);
        Opened::created(panel, branch, true, Vec::new())
    }

    /// `panel`'s folder was replaced by a different one with the same
    /// name: its view settings stay, but its cursor, scroll position and
    /// selection belonged to the old folder and are dropped, and the panels
    /// below it (inside the old folder) are marked unavailable. Returns
    /// the panel and its descendants, whose listings must be read again.
    fn reset_replaced(&mut self, panel: PanelId, identity: DirectoryIdentity) -> Vec<PanelId> {
        let below = self.descendants(panel);
        if let Some(p) = self.panels.get_mut(&panel) {
            p.state = p.state.inherit();
            p.identity = Some(identity);
            p.unavailable = None;
        }
        for id in below.iter().skip(1) {
            if let Some(p) = self.panels.get_mut(id) {
                p.unavailable = Some(Unavailability::Missing);
            }
        }
        below
    }

    /// `panel`'s folder was renamed to `location`: it and every panel
    /// below it take the new path (cursors, selections and scroll anchors
    /// inside it too). Returns the moved panels.
    fn relocate(&mut self, panel: PanelId, location: &Location) -> Vec<PanelId> {
        let old = self.panels[&panel].location.clone();
        let moved = self.descendants(panel);
        for id in &moved {
            let p = self.panels.get_mut(id).expect("descendant exists");
            p.location = rebase(&p.location, &old, location);
            let state = &mut p.state;
            if let Some(cursor) = &mut state.cursor {
                *cursor = rebase(cursor, &old, location);
            }
            if let Some(scroll) = &mut state.scroll {
                scroll.item = rebase(&scroll.item, &old, location);
            }
            let items: Vec<Location> = state
                .selection
                .items()
                .iter()
                .map(|l| rebase(l, &old, location))
                .collect();
            let anchor = state.selection.anchor().map(|a| rebase(a, &old, location));
            state.selection.replace(items);
            state.selection.set_anchor(anchor);
        }
        moved
    }
}

/// `location` with its `old` prefix replaced by `new` (after a rename).
/// Locations outside `old` are returned unchanged.
fn rebase(location: &Location, old: &Location, new: &Location) -> Location {
    match (location, old, new) {
        (Location::Path(p), Location::Path(o), Location::Path(n)) => match p.strip_prefix(o) {
            Ok(rest) if rest.as_os_str().is_empty() => Location::Path(n.clone()),
            Ok(rest) => Location::Path(n.join(rest)),
            Err(_) => location.clone(),
        },
        (Location::Trash(p), Location::Trash(o), Location::Trash(n)) => match p.strip_prefix(o) {
            Ok(rest) if rest.as_os_str().is_empty() => Location::Trash(n.clone()),
            Ok(rest) => Location::Trash(n.join(rest)),
            Err(_) => location.clone(),
        },
        _ => location.clone(),
    }
}
