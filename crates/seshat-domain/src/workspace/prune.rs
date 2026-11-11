//! Backward branch pruning (`db`, `dB`) and putting pruned panels back
//! (workspace undo). See ADR 0007 and decisions B1–B9.
//!
//! Pruning changes the workspace structure only. It never touches files:
//! nothing here knows about the filesystem.

use std::collections::HashSet;

use thiserror::Error;

use super::model::Workspace;
use super::{Branch, BranchId, Panel, PanelId};

/// How far back a prune reaches (decision B1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneMotion {
    /// `b`: one navigation step, back to the active panel's parent.
    Step,
    /// `B`: one branch segment, back to the nearest panel that another
    /// branch also passes through, or to the branch's root if it shares
    /// nothing.
    Segment,
}

/// Why a prune request was refused. A refused prune changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PruneRefusal {
    /// The request named a branch or panel that is no longer active.
    #[error("the workspace changed before the prune ran")]
    Stale,
    /// The active panel is a root: there is nothing behind it.
    #[error("there is nothing behind this panel to prune back to")]
    NoDestination,
    /// Panels of the active branch continue beyond the active panel
    /// (decision B2).
    #[error("panels continue beyond this one")]
    PanelsBeyond,
    /// Other branches pass through the active panel.
    #[error("{branches} branches pass through this panel")]
    Shared { branches: usize },
}

/// Why pruned panels could not be put back. Nothing is changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RestoreRefusal {
    /// The panel the prune went back to has been closed since.
    #[error("the panel it was pruned back to is gone")]
    DestinationGone,
    /// The workspace changed there since, e.g. the folder was reopened.
    #[error("the workspace has changed there since")]
    Changed,
}

/// What a prune would do, worked out without changing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePlan {
    /// The panel that is kept and becomes active.
    pub destination: PanelId,
    /// The panels removed, from the destination's child down to the
    /// active panel.
    pub removed: Vec<PanelId>,
    /// The destination is shared with other branches, so the active
    /// branch has no panels of its own left and ends (decision B3).
    pub ends_branch: bool,
}

/// A completed prune: enough to put everything back exactly as it was
/// (decision B5). The removed panels keep their ids, which are never
/// reused, so restoring them cannot collide with newer panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunedSegment {
    destination: PanelId,
    /// Where the first removed panel sat among the destination's children.
    index: usize,
    /// The removed panels with their state, top-down.
    panels: Vec<Panel>,
    /// The pruned branch as it was.
    branch: Branch,
    ends_branch: bool,
    /// When the branch ended: the branch that took over and its cursor
    /// before it moved to the destination.
    adopter: Option<(BranchId, PanelId)>,
}

impl PrunedSegment {
    pub fn destination(&self) -> PanelId {
        self.destination
    }

    pub fn removed(&self) -> Vec<PanelId> {
        self.panels.iter().map(Panel::id).collect()
    }

    /// The branch the prune was made on.
    pub fn branch(&self) -> BranchId {
        self.branch.id
    }

    pub fn ends_branch(&self) -> bool {
        self.ends_branch
    }

    /// Selected items held by the removed panels; they leave the
    /// workspace-wide selection with them (decision B6).
    pub fn deselected(&self) -> usize {
        self.panels.iter().map(|p| p.state.selection.len()).sum()
    }
}

impl Workspace {
    /// Works out a prune of `branch` from `panel` without changing
    /// anything. Both must be the active branch and panel.
    ///
    /// Only the active branch's tip can be pruned (decision B2), so every
    /// removed panel has exactly one child: no other branch can pass
    /// through them, and shared ancestors are never removed.
    pub fn resolve_prune(
        &self,
        branch: BranchId,
        panel: PanelId,
        motion: PruneMotion,
    ) -> Result<PrunePlan, PruneRefusal> {
        if branch != self.active || panel != self.active_panel() {
            return Err(PruneRefusal::Stale);
        }
        let active = &self.panels[&panel];
        if !active.is_leaf() {
            let branches = self.branches_through(panel).len();
            return Err(if branches > 1 {
                PruneRefusal::Shared { branches }
            } else {
                PruneRefusal::PanelsBeyond
            });
        }
        let mut destination = active.parent().ok_or(PruneRefusal::NoDestination)?;
        if motion == PruneMotion::Segment {
            loop {
                let p = &self.panels[&destination];
                match p.parent() {
                    Some(parent) if p.children().len() < 2 => destination = parent,
                    _ => break,
                }
            }
        }
        let mut removed: Vec<PanelId> = self
            .ancestors_or_self(panel)
            .into_iter()
            .take_while(|&p| p != destination)
            .collect();
        removed.reverse();
        Ok(PrunePlan {
            destination,
            removed,
            ends_branch: self.panels[&destination].children().len() > 1,
        })
    }

    /// Prunes `branch` back from `panel` by `motion` (spec 3 §4–§9).
    ///
    /// The destination keeps its state and becomes active. If the
    /// destination is shared, the active branch ends and the most recently
    /// active branch through the destination takes over there. Other
    /// branches keep their panels, ids and state, and Focus Mode is never
    /// changed (decision B4). Removed panels leave the jump list.
    pub fn prune(
        &mut self,
        branch: BranchId,
        panel: PanelId,
        motion: PruneMotion,
    ) -> Result<PrunedSegment, PruneRefusal> {
        let plan = self.resolve_prune(branch, panel, motion)?;
        let destination = plan.destination;
        let first = plan.removed[0];
        let index = self.panels[&destination]
            .children
            .iter()
            .position(|&c| c == first)
            .expect("the removed chain hangs off the destination");
        self.panels
            .get_mut(&destination)
            .expect("destination exists")
            .children
            .remove(index);
        let panels: Vec<Panel> = plan
            .removed
            .iter()
            .map(|id| self.panels.remove(id).expect("removed panel exists"))
            .collect();
        let gone: HashSet<PanelId> = plan.removed.iter().copied().collect();
        self.back.retain(|p| !gone.contains(p));
        self.forward.retain(|p| !gone.contains(p));
        self.back.dedup();
        self.forward.dedup();

        let pruned_branch = self.branches[&branch].clone();
        let adopter = if plan.ends_branch {
            self.branches.remove(&branch);
            let adopter = *self
                .branches_through(destination)
                .first()
                .expect("a shared destination lies on another branch");
            let previous = self.branches[&adopter].cursor;
            self.set_active_keeping_presentation(adopter, destination, false);
            Some((adopter, previous))
        } else {
            let b = self.branches.get_mut(&branch).expect("active branch");
            b.tip = destination;
            self.set_active_keeping_presentation(branch, destination, false);
            None
        };
        Ok(PrunedSegment {
            destination,
            index,
            panels,
            branch: pruned_branch,
            ends_branch: plan.ends_branch,
            adopter,
        })
    }

    /// Puts the panels of `pruned` back where they were, with their ids and
    /// state, and makes the pruned branch active again at its old panel.
    /// The branch that took over gets its old position back if it still
    /// sits where the prune left it. Focus Mode is never changed.
    ///
    /// Refused, changing nothing, if the workspace changed there since:
    /// the destination is gone, a panel for the same folder was opened
    /// under it, or (for a branch that was not ended) the branch has been
    /// extended or closed.
    pub fn restore_pruned(&mut self, pruned: &PrunedSegment) -> Result<(), RestoreRefusal> {
        let destination = self
            .panels
            .get(&pruned.destination)
            .ok_or(RestoreRefusal::DestinationGone)?;
        let first = &pruned.panels[0];
        let reopened = destination
            .children()
            .iter()
            .any(|c| self.panels[c].location() == first.location());
        let ids_free = pruned
            .panels
            .iter()
            .all(|p| !self.panels.contains_key(&p.id));
        let shape_matches = if pruned.ends_branch {
            // The destination must still be shared; were it a leaf now, it
            // would be another branch's tip.
            !destination.is_leaf() && !self.branches.contains_key(&pruned.branch.id)
        } else {
            self.branches
                .get(&pruned.branch.id)
                .is_some_and(|b| b.tip == pruned.destination)
        };
        if reopened || !ids_free || !shape_matches {
            return Err(RestoreRefusal::Changed);
        }

        let index = pruned.index.min(destination.children().len());
        self.panels
            .get_mut(&pruned.destination)
            .expect("checked above")
            .children
            .insert(index, first.id);
        for panel in &pruned.panels {
            self.panels.insert(panel.id, panel.clone());
        }
        let branch = &pruned.branch;
        if pruned.ends_branch {
            self.branches.insert(branch.id, branch.clone());
            if let Some((adopter, previous)) = pruned.adopter
                && let Some(b) = self.branches.get(&adopter)
                && b.cursor == pruned.destination
                && self.ancestors_or_self(b.tip).contains(&previous)
            {
                self.branches.get_mut(&adopter).expect("checked").cursor = previous;
            }
        } else {
            self.branches.get_mut(&branch.id).expect("checked").tip = branch.tip;
        }
        self.set_active_keeping_presentation(branch.id, branch.cursor, true);
        Ok(())
    }
}
