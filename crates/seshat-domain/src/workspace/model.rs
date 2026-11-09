use std::collections::{BTreeMap, HashSet};

use thiserror::Error;

use super::{Branch, BranchId, Panel, PanelId, PanelState, Unavailability};
use crate::entities::Location;

/// Back/forward history keeps at most this many panel activations.
const JUMP_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceError {
    #[error("no such panel: {0}")]
    UnknownPanel(PanelId),
    #[error("no such branch: {0}")]
    UnknownBranch(BranchId),
    #[error("the last branch cannot be closed")]
    LastBranch,
    #[error("this folder has no parent")]
    NoParent,
    #[error("only a root panel can get a new parent")]
    NotARoot,
    #[error("inconsistent workspace: {0}")]
    Inconsistent(String),
}

/// Result of opening a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opened {
    pub panel: PanelId,
    /// A new panel was created (otherwise an existing one was activated).
    pub created: bool,
    /// A new branch was created for it.
    pub new_branch: bool,
}

/// The complete spatial workspace: a forest of persistent directory panels
/// and the branches (root-to-leaf paths) through it.
///
/// Invariants (checked by [`Workspace::check_invariants`]):
/// - panels form a forest whose roots are listed in `roots`;
/// - every leaf panel is the tip of exactly one branch, and every branch
///   tip is a leaf, so branches never duplicate shared ancestors;
/// - each branch's cursor lies on its own path;
/// - siblings never show the same location;
/// - the active panel is the active branch's cursor.
///
/// Navigation never changes presentation state, and presentation (Focus
/// Mode) never changes panels or branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    panels: BTreeMap<PanelId, Panel>,
    roots: Vec<PanelId>,
    branches: BTreeMap<BranchId, Branch>,
    active: BranchId,
    focus_mode: bool,
    /// Workspace preference: keep Focus Mode when switching branches.
    retain_focus_on_branch_switch: bool,
    back: Vec<PanelId>,
    forward: Vec<PanelId>,
    next_id: u64,
    clock: u64,
}

/// A workspace taken apart, for persistence. [`Workspace::from_parts`]
/// validates it before use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceParts {
    pub panels: Vec<PanelParts>,
    pub roots: Vec<PanelId>,
    pub branches: Vec<BranchParts>,
    pub active: BranchId,
    pub focus_mode: bool,
    pub retain_focus_on_branch_switch: bool,
    pub back: Vec<PanelId>,
    pub forward: Vec<PanelId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelParts {
    pub id: PanelId,
    pub location: Location,
    pub parent: Option<PanelId>,
    pub children: Vec<PanelId>,
    pub state: PanelState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchParts {
    pub id: BranchId,
    pub tip: PanelId,
    pub cursor: PanelId,
    pub created: u64,
    pub last_active: u64,
}

impl Workspace {
    /// A workspace with a single root panel and branch.
    pub fn new(location: Location, state: PanelState) -> Self {
        let mut workspace = Self {
            panels: BTreeMap::new(),
            roots: Vec::new(),
            branches: BTreeMap::new(),
            active: BranchId::from_raw(0),
            focus_mode: false,
            retain_focus_on_branch_switch: true,
            back: Vec::new(),
            forward: Vec::new(),
            next_id: 1,
            clock: 0,
        };
        let panel = workspace.add_panel(location, None, state);
        workspace.roots.push(panel);
        let branch = workspace.add_branch(panel);
        workspace.active = branch;
        workspace
    }

    // ── Queries ─────────────────────────────────────────────────────────

    pub fn panel(&self, id: PanelId) -> Option<&Panel> {
        self.panels.get(&id)
    }

    /// Mutable access to a panel's own state. Its place in the tree can
    /// only change through workspace operations.
    pub fn state_mut(&mut self, id: PanelId) -> Option<&mut PanelState> {
        self.panels.get_mut(&id).map(|p| &mut p.state)
    }

    pub fn panels(&self) -> impl Iterator<Item = &Panel> {
        self.panels.values()
    }

    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    pub fn roots(&self) -> &[PanelId] {
        &self.roots
    }

    pub fn branch(&self, id: BranchId) -> Option<&Branch> {
        self.branches.get(&id)
    }

    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    pub fn active_branch(&self) -> &Branch {
        &self.branches[&self.active]
    }

    pub fn active_panel(&self) -> PanelId {
        self.active_branch().cursor
    }

    pub fn focus_mode(&self) -> bool {
        self.focus_mode
    }

    pub fn retain_focus_on_branch_switch(&self) -> bool {
        self.retain_focus_on_branch_switch
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Branches in spatial order: depth-first, left to right, which is the
    /// order `[` and `]` cycle through.
    pub fn branches(&self) -> Vec<&Branch> {
        let mut leaves = Vec::new();
        for &root in &self.roots {
            self.collect_leaves(root, &mut leaves);
        }
        leaves
            .into_iter()
            .filter_map(|leaf| self.branch_with_tip(leaf))
            .collect()
    }

    /// The panels of `branch`, from its root to its tip.
    pub fn path(&self, branch: BranchId) -> Vec<PanelId> {
        self.branches
            .get(&branch)
            .map(|b| self.path_to(b.tip))
            .unwrap_or_default()
    }

    pub fn active_path(&self) -> Vec<PanelId> {
        self.path(self.active)
    }

    /// Branches whose path includes `panel`, most recently active first.
    pub fn branches_through(&self, panel: PanelId) -> Vec<BranchId> {
        let mut through: Vec<&Branch> = self
            .branches
            .values()
            .filter(|b| self.ancestors_or_self(b.tip).contains(&panel))
            .collect();
        through.sort_by_key(|b| std::cmp::Reverse(b.last_active));
        through.into_iter().map(|b| b.id).collect()
    }

    /// `panel` and every panel below it.
    pub fn descendants(&self, panel: PanelId) -> Vec<PanelId> {
        let mut out = Vec::new();
        let mut stack = vec![panel];
        while let Some(id) = stack.pop() {
            if let Some(p) = self.panels.get(&id) {
                out.push(id);
                stack.extend(p.children.iter().rev());
            }
        }
        out
    }

    /// The closest panel above `panel` that is not marked unavailable.
    pub fn nearest_available_ancestor(&self, panel: PanelId) -> Option<PanelId> {
        self.ancestors_or_self(panel)
            .into_iter()
            .skip(1)
            .find(|id| self.panels[id].unavailable.is_none())
    }

    /// Panels showing `root` or something beneath it (e.g. a device).
    pub fn panels_within(&self, root: &std::path::Path) -> Vec<PanelId> {
        self.panels
            .values()
            .filter(|p| p.location.is_within(root))
            .map(|p| p.id)
            .collect()
    }

    /// The panel on the active branch showing `location`, if any.
    pub fn find_on_active_path(&self, location: &Location) -> Option<PanelId> {
        self.active_path()
            .into_iter()
            .find(|id| self.panels[id].location == *location)
    }

    // ── Navigation ──────────────────────────────────────────────────────

    /// Makes `panel` the active panel. If the active branch passes through
    /// it, that branch stays active; otherwise the most recently active
    /// branch through it becomes active. Returns whether anything changed.
    pub fn activate_panel(&mut self, panel: PanelId) -> Result<bool, WorkspaceError> {
        self.activate_panel_inner(panel, true)
    }

    /// Moves to the previous panel (towards the root) of the active branch.
    /// Never creates panels.
    pub fn move_left(&mut self) -> bool {
        let cursor = self.active_panel();
        match self.panels[&cursor].parent {
            Some(parent) => self.set_active(self.active, parent, true),
            None => false,
        }
    }

    /// Moves to the next existing panel (towards the tip) of the active
    /// branch. Never creates panels or branches.
    pub fn move_right(&mut self) -> bool {
        let path = self.active_path();
        let cursor = self.active_panel();
        match path.iter().position(|&p| p == cursor) {
            Some(i) if i + 1 < path.len() => self.set_active(self.active, path[i + 1], true),
            _ => false,
        }
    }

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
        let parent = self
            .panels
            .get(&from)
            .ok_or(WorkspaceError::UnknownPanel(from))?;
        if let Some(&existing) = parent
            .children
            .iter()
            .find(|c| self.panels[c].location == location)
        {
            self.activate_panel_inner(existing, true)?;
            return Ok(Opened {
                panel: existing,
                created: false,
                new_branch: false,
            });
        }
        let state = state.unwrap_or_else(|| parent.state.inherit());
        let extends = self.branch_with_tip(from).map(|b| b.id);
        let child = self.add_panel(location, Some(from), state);
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
        Ok(Opened {
            panel: child,
            created: true,
            new_branch,
        })
    }

    /// Opens `location` from outside the tree (sidebar, address bar,
    /// devices). If the active branch already shows it, that panel is
    /// activated; otherwise it becomes a new root with its own branch.
    pub fn open_root(&mut self, location: Location, state: PanelState) -> Opened {
        if let Some(existing) = self.find_on_active_path(&location) {
            self.set_active(self.active, existing, true);
            return Opened {
                panel: existing,
                created: false,
                new_branch: false,
            };
        }
        let panel = self.add_panel(location, None, state);
        self.roots.push(panel);
        let branch = self.add_branch(panel);
        self.set_active(branch, panel, true);
        Opened {
            panel,
            created: true,
            new_branch: true,
        }
    }

    /// Gives root panel `root` a parent panel for its parent folder and
    /// activates it. Every branch through `root` now passes through the
    /// new parent; nothing else changes.
    pub fn insert_parent(&mut self, root: PanelId) -> Result<PanelId, WorkspaceError> {
        let panel = self
            .panels
            .get(&root)
            .ok_or(WorkspaceError::UnknownPanel(root))?;
        if !panel.is_root() {
            return Err(WorkspaceError::NotARoot);
        }
        let location = panel.location.parent().ok_or(WorkspaceError::NoParent)?;
        let state = panel.state.inherit();
        let parent = self.add_panel(location, None, state);
        self.panels.get_mut(&parent).expect("just added").children = vec![root];
        self.panels.get_mut(&root).expect("checked").parent = Some(parent);
        let index = self.roots.iter().position(|&r| r == root).expect("is root");
        self.roots[index] = parent;
        self.activate_panel_inner(parent, true)?;
        Ok(parent)
    }

    pub fn activate_branch(&mut self, branch: BranchId) -> Result<bool, WorkspaceError> {
        let cursor = self
            .branches
            .get(&branch)
            .ok_or(WorkspaceError::UnknownBranch(branch))?
            .cursor;
        Ok(self.set_active(branch, cursor, true))
    }

    /// Switches to the next (or previous) branch in spatial order,
    /// wrapping around. Returns whether the active branch changed.
    pub fn cycle_branch(&mut self, forward: bool) -> bool {
        let order: Vec<BranchId> = self.branches().iter().map(|b| b.id).collect();
        if order.len() < 2 {
            return false;
        }
        let index = order.iter().position(|&b| b == self.active).unwrap_or(0);
        let next = if forward {
            (index + 1) % order.len()
        } else {
            (index + order.len() - 1) % order.len()
        };
        let cursor = self.branches[&order[next]].cursor;
        self.set_active(order[next], cursor, true)
    }

    /// Closes `branch`: removes the panels only it uses, keeping shared
    /// ancestors and anything other branches use. If it was active, the
    /// most recently active remaining branch takes over. The last branch
    /// cannot be closed. Returns the removed panels.
    pub fn close_branch(&mut self, branch: BranchId) -> Result<Vec<PanelId>, WorkspaceError> {
        let tip = self
            .branches
            .get(&branch)
            .ok_or(WorkspaceError::UnknownBranch(branch))?
            .tip;
        if self.branches.len() == 1 {
            return Err(WorkspaceError::LastBranch);
        }
        let mut removed = Vec::new();
        let mut current = tip;
        loop {
            let panel = self.panels.remove(&current).expect("path panel exists");
            removed.push(current);
            match panel.parent {
                Some(parent) => {
                    let parent_panel = self.panels.get_mut(&parent).expect("parent exists");
                    parent_panel.children.retain(|&c| c != current);
                    if !parent_panel.children.is_empty() {
                        break; // shared ancestor: other branches still use it
                    }
                    current = parent;
                }
                None => {
                    self.roots.retain(|&r| r != current);
                    break;
                }
            }
        }
        self.branches.remove(&branch);
        let gone: HashSet<PanelId> = removed.iter().copied().collect();
        self.back.retain(|p| !gone.contains(p));
        self.forward.retain(|p| !gone.contains(p));
        self.back.dedup();
        self.forward.dedup();
        if self.active == branch {
            let next = self
                .branches
                .values()
                .max_by_key(|b| b.last_active)
                .expect("another branch remains");
            let (id, cursor) = (next.id, next.cursor);
            self.set_active(id, cursor, false);
        }
        Ok(removed)
    }

    /// Goes back to the previously active panel (jump list).
    pub fn go_back(&mut self) -> bool {
        self.jump(true)
    }

    pub fn go_forward(&mut self) -> bool {
        self.jump(false)
    }

    // ── Presentation and availability ───────────────────────────────────

    /// Focus Mode is presentation only: it never touches panels, branches
    /// or the active panel. Returns whether it changed.
    pub fn set_focus_mode(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.focus_mode, on) != on
    }

    pub fn set_retain_focus_on_branch_switch(&mut self, retain: bool) {
        self.retain_focus_on_branch_switch = retain;
    }

    /// Marks `panel` and everything below it unavailable. Returns them.
    pub fn mark_unavailable(&mut self, panel: PanelId, reason: Unavailability) -> Vec<PanelId> {
        let affected = self.descendants(panel);
        for id in &affected {
            if let Some(p) = self.panels.get_mut(id) {
                p.unavailable = Some(reason);
            }
        }
        affected
    }

    pub fn mark_available(&mut self, panel: PanelId) {
        if let Some(p) = self.panels.get_mut(&panel) {
            p.unavailable = None;
        }
    }

    // ── Persistence ─────────────────────────────────────────────────────

    pub fn to_parts(&self) -> WorkspaceParts {
        WorkspaceParts {
            panels: self
                .panels
                .values()
                .map(|p| PanelParts {
                    id: p.id,
                    location: p.location.clone(),
                    parent: p.parent,
                    children: p.children.clone(),
                    state: p.state.clone(),
                })
                .collect(),
            roots: self.roots.clone(),
            branches: self
                .branches
                .values()
                .map(|b| BranchParts {
                    id: b.id,
                    tip: b.tip,
                    cursor: b.cursor,
                    created: b.created,
                    last_active: b.last_active,
                })
                .collect(),
            active: self.active,
            focus_mode: self.focus_mode,
            retain_focus_on_branch_switch: self.retain_focus_on_branch_switch,
            back: self.back.clone(),
            forward: self.forward.clone(),
        }
    }

    /// Rebuilds a workspace, rejecting anything that breaks an invariant.
    pub fn from_parts(parts: WorkspaceParts) -> Result<Self, WorkspaceError> {
        let max_id = parts
            .panels
            .iter()
            .map(|p| p.id.raw())
            .chain(parts.branches.iter().map(|b| b.id.raw()))
            .max()
            .unwrap_or(0);
        let clock = parts
            .branches
            .iter()
            .map(|b| b.created.max(b.last_active))
            .max()
            .unwrap_or(0);
        let mut panels = BTreeMap::new();
        for p in parts.panels {
            let duplicate = panels
                .insert(
                    p.id,
                    Panel {
                        id: p.id,
                        location: p.location,
                        parent: p.parent,
                        children: p.children,
                        unavailable: None,
                        state: p.state,
                    },
                )
                .is_some();
            if duplicate {
                return Err(WorkspaceError::Inconsistent(format!(
                    "duplicate panel {}",
                    p.id
                )));
            }
        }
        let mut branches = BTreeMap::new();
        for b in parts.branches {
            branches.insert(
                b.id,
                Branch {
                    id: b.id,
                    tip: b.tip,
                    cursor: b.cursor,
                    created: b.created,
                    last_active: b.last_active,
                },
            );
        }
        let workspace = Self {
            panels,
            roots: parts.roots,
            branches,
            active: parts.active,
            focus_mode: parts.focus_mode,
            retain_focus_on_branch_switch: parts.retain_focus_on_branch_switch,
            back: parts.back,
            forward: parts.forward,
            next_id: max_id + 1,
            clock,
        };
        workspace
            .check_invariants()
            .map_err(WorkspaceError::Inconsistent)?;
        Ok(workspace)
    }

    /// Verifies every structural invariant. Used by tests after each
    /// operation and when restoring a persisted workspace.
    pub fn check_invariants(&self) -> Result<(), String> {
        if self.roots.is_empty() || self.branches.is_empty() {
            return Err("a workspace needs at least one root and branch".into());
        }
        let mut seen = HashSet::new();
        let mut stack: Vec<(PanelId, Option<PanelId>)> =
            self.roots.iter().map(|&r| (r, None)).collect();
        while let Some((id, parent)) = stack.pop() {
            let panel = self.panels.get(&id).ok_or(format!("missing panel {id}"))?;
            if !seen.insert(id) {
                return Err(format!("panel {id} is reachable twice"));
            }
            if panel.parent != parent {
                return Err(format!("panel {id} has the wrong parent"));
            }
            let mut locations = HashSet::new();
            for &child in &panel.children {
                let c = self
                    .panels
                    .get(&child)
                    .ok_or(format!("missing child {child}"))?;
                if !locations.insert(&c.location) {
                    return Err(format!("panel {id} has duplicate children"));
                }
                stack.push((child, Some(id)));
            }
        }
        if seen.len() != self.panels.len() {
            return Err("some panels are not reachable from a root".into());
        }
        let leaves: HashSet<PanelId> = self
            .panels
            .values()
            .filter(|p| p.is_leaf())
            .map(|p| p.id)
            .collect();
        let tips: Vec<PanelId> = self.branches.values().map(|b| b.tip).collect();
        let tip_set: HashSet<PanelId> = tips.iter().copied().collect();
        if tip_set.len() != tips.len() || tip_set != leaves {
            return Err("branches and leaf panels do not match one to one".into());
        }
        for b in self.branches.values() {
            if !self.ancestors_or_self(b.tip).contains(&b.cursor) {
                return Err(format!("branch {} has its cursor off its path", b.id));
            }
            if b.id.raw() >= self.next_id {
                return Err(format!("branch {} is above the id counter", b.id));
            }
        }
        if !self.branches.contains_key(&self.active) {
            return Err("the active branch does not exist".into());
        }
        if self.panels.keys().any(|id| id.raw() >= self.next_id) {
            return Err("a panel is above the id counter".into());
        }
        if self
            .back
            .iter()
            .chain(&self.forward)
            .any(|p| !self.panels.contains_key(p))
        {
            return Err("the jump list refers to a missing panel".into());
        }
        Ok(())
    }

    // ── Internals ───────────────────────────────────────────────────────

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn add_panel(&mut self, location: Location, parent: Option<PanelId>, state: PanelState) -> PanelId {
        let id = PanelId::from_raw(self.fresh_id());
        self.panels.insert(
            id,
            Panel {
                id,
                location,
                parent,
                children: Vec::new(),
                unavailable: None,
                state,
            },
        );
        id
    }

    fn add_branch(&mut self, tip: PanelId) -> BranchId {
        let id = BranchId::from_raw(self.fresh_id());
        let now = self.tick();
        self.branches.insert(
            id,
            Branch {
                id,
                tip,
                cursor: tip,
                created: now,
                last_active: now,
            },
        );
        id
    }

    fn branch_with_tip(&self, tip: PanelId) -> Option<&Branch> {
        self.branches.values().find(|b| b.tip == tip)
    }

    fn collect_leaves(&self, panel: PanelId, out: &mut Vec<PanelId>) {
        let p = &self.panels[&panel];
        if p.children.is_empty() {
            out.push(panel);
        }
        for &child in &p.children {
            self.collect_leaves(child, out);
        }
    }

    /// `panel`, its parent, and so on up to the root.
    fn ancestors_or_self(&self, panel: PanelId) -> Vec<PanelId> {
        let mut out = Vec::new();
        let mut current = Some(panel);
        while let Some(id) = current {
            let Some(p) = self.panels.get(&id) else {
                break;
            };
            out.push(id);
            current = p.parent;
        }
        out
    }

    fn path_to(&self, tip: PanelId) -> Vec<PanelId> {
        let mut path = self.ancestors_or_self(tip);
        path.reverse();
        path
    }

    fn activate_panel_inner(&mut self, panel: PanelId, record: bool) -> Result<bool, WorkspaceError> {
        if !self.panels.contains_key(&panel) {
            return Err(WorkspaceError::UnknownPanel(panel));
        }
        let branch = if self.active_path().contains(&panel) {
            self.active
        } else {
            *self
                .branches_through(panel)
                .first()
                .expect("every panel lies on a branch")
        };
        Ok(self.set_active(branch, panel, record))
    }

    /// Makes `branch` active with its cursor on `panel`. Records the
    /// previous active panel in the jump list when `record` is set.
    fn set_active(&mut self, branch: BranchId, panel: PanelId, record: bool) -> bool {
        // The previous active branch may just have been closed.
        let before = self.branches.get(&self.active).map(|b| (b.id, b.cursor));
        let now = self.tick();
        if branch != self.active && !self.retain_focus_on_branch_switch {
            self.focus_mode = false;
        }
        self.active = branch;
        let b = self.branches.get_mut(&branch).expect("branch exists");
        b.cursor = panel;
        b.last_active = now;
        if let Some((_, previous)) = before
            && record
            && previous != panel
        {
            self.back.push(previous);
            if self.back.len() > JUMP_LIMIT {
                self.back.remove(0);
            }
            self.forward.clear();
        }
        before != Some((branch, panel))
    }

    fn jump(&mut self, back: bool) -> bool {
        loop {
            let target = if back {
                self.back.pop()
            } else {
                self.forward.pop()
            };
            let Some(target) = target else {
                return false;
            };
            if !self.panels.contains_key(&target) || target == self.active_panel() {
                continue;
            }
            let current = self.active_panel();
            if back {
                self.forward.push(current);
            } else {
                self.back.push(current);
            }
            self.activate_panel_inner(target, false)
                .expect("jump target exists");
            return true;
        }
    }
}
