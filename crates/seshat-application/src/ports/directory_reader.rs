use seshat_domain::entities::{FileEntry, Location};
use thiserror::Error;

use crate::LocalBoxFuture;

/// Why a directory could not be listed. Kept coarse so presentation can map
/// each variant to a clear message.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListingError {
    #[error("location does not exist")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
    #[error("not a directory")]
    NotADirectory,
    #[error("location is unavailable: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Other(String),
}

/// Enumerates the entries of a location without reading file contents.
pub trait DirectoryReader {
    fn read_directory<'a>(
        &'a self,
        location: &'a Location,
    ) -> LocalBoxFuture<'a, Result<Vec<FileEntry>, ListingError>>;
}
