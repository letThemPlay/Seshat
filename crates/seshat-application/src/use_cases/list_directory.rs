use std::rc::Rc;

use seshat_domain::entities::{FileEntry, Location};

use crate::ports::{DirectoryReader, ListingError};

/// Reads a location's entries. Filtering, searching and sorting happen in
/// memory afterwards (see [`crate::view::arrange`]), so changing them never
/// touches the disk again.
#[derive(Clone)]
pub struct ListDirectory {
    reader: Rc<dyn DirectoryReader>,
}

impl std::fmt::Debug for ListDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListDirectory").finish_non_exhaustive()
    }
}

impl ListDirectory {
    pub fn new(reader: Rc<dyn DirectoryReader>) -> Self {
        Self { reader }
    }

    pub async fn execute(&self, location: &Location) -> Result<Vec<FileEntry>, ListingError> {
        self.reader.read_directory(location).await
    }
}
