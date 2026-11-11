//! The spatial workspace as application services: navigation, panel
//! listings and selection ([`WorkspaceService`]), cross-panel transfers
//! ([`TransferPlan`]), the persistence port ([`WorkspaceStore`]) and
//! workspace undo for backward pruning ([`WorkspaceChange`]).

mod listing;
mod service;
mod store;
#[cfg(test)]
mod tests;
mod transfer;
mod undo;

pub use listing::{Content, PanelListing};
pub use service::{DEFAULT_LISTING_CAPACITY, Effect, SelectionGroup, WorkspaceService};
pub use store::{WORKSPACE_SCHEMA_VERSION, WorkspaceStore};
pub use transfer::{TransferError, TransferKind, TransferPlan};
pub use undo::{PruneError, PruneOutcome, UNDO_LIMIT, UndoError, UndoOutcome, WorkspaceChange};
