//! The workspace taken apart for persistence, and put back together with
//! every invariant checked.

use std::collections::BTreeMap;

use super::{Workspace, WorkspaceError};
use crate::entities::{DirectoryIdentity, Location};
use crate::workspace::{Branch, BranchId, JumpList, Panel, PanelId, PanelState};

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
    /// The folder's identity when last opened, if known.
    pub identity: Option<DirectoryIdentity>,
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
                    identity: p.identity,
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
            back: self.jumps.back(),
            forward: self.jumps.forward(),
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
                        identity: p.identity,
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
            jumps: JumpList::from_parts(parts.back, parts.forward),
            next_id: max_id + 1,
            clock,
        };
        workspace
            .check_invariants()
            .map_err(WorkspaceError::Inconsistent)?;
        Ok(workspace)
    }
}
