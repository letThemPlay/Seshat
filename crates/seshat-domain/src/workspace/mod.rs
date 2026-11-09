//! The spatial workspace: persistent directory panels arranged in a tree,
//! and first-class branches (root-to-leaf navigation paths) through it.
//!
//! This is navigation state only. File operations and their lifecycle live
//! in the operation manager; how panels are drawn (including Focus Mode)
//! is up to the presentation layer, which reads this model.

mod ids;
mod model;
mod panel;
#[cfg(test)]
mod tests;

pub use ids::{BranchId, PanelId};
pub use model::{BranchParts, Opened, PanelParts, Workspace, WorkspaceError, WorkspaceParts};
pub use panel::{Branch, Panel, PanelState, ScrollAnchor, Selection, Unavailability};
