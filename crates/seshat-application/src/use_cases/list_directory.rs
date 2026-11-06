use std::cmp::Ordering;
use std::rc::Rc;

use seshat_domain::entities::{FileEntry, Location};

use crate::ports::{DirectoryReader, ListingError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListingOptions {
    pub show_hidden: bool,
}

/// Lists a location for display: filters hidden entries and sorts with
/// directories first, then by case-insensitive name.
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

    pub async fn execute(
        &self,
        location: &Location,
        options: ListingOptions,
    ) -> Result<Vec<FileEntry>, ListingError> {
        let mut entries = self.reader.read_directory(location).await?;
        if !options.show_hidden {
            entries.retain(|e| !e.hidden);
        }
        entries.sort_by(compare_entries);
        Ok(entries)
    }
}

fn compare_entries(a: &FileEntry, b: &FileEntry) -> Ordering {
    b.is_directory()
        .cmp(&a.is_directory())
        .then_with(|| {
            let a_name = a.name.to_string_lossy().to_lowercase();
            let b_name = b.name.to_string_lossy().to_lowercase();
            a_name.cmp(&b_name)
        })
        .then_with(|| a.name.cmp(&b.name))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use seshat_domain::entities::EntryKind;

    use super::*;
    use crate::LocalBoxFuture;

    struct FakeReader(Result<Vec<FileEntry>, ListingError>);

    impl DirectoryReader for FakeReader {
        fn read_directory<'a>(
            &'a self,
            _location: &'a Location,
        ) -> LocalBoxFuture<'a, Result<Vec<FileEntry>, ListingError>> {
            let result = self.0.clone();
            Box::pin(async move { result })
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                return output;
            }
        }
    }

    fn entry(name: &str, kind: EntryKind, hidden: bool) -> FileEntry {
        FileEntry {
            name: OsString::from(name),
            path: PathBuf::from("/x").join(name),
            kind,
            size: None,
            modified: None,
            hidden,
        }
    }

    fn names(entries: &[FileEntry]) -> Vec<String> {
        entries.iter().map(FileEntry::display_name).collect()
    }

    #[test]
    fn sorts_directories_first_then_case_insensitively() {
        let reader = FakeReader(Ok(vec![
            entry("b.txt", EntryKind::File, false),
            entry("Zeta", EntryKind::Directory, false),
            entry("A.txt", EntryKind::File, false),
            entry("alpha", EntryKind::Directory, false),
        ]));
        let use_case = ListDirectory::new(Rc::new(reader));
        let result = block_on(use_case.execute(&Location::Trash, ListingOptions::default()));
        assert_eq!(names(&result.unwrap()), ["alpha", "Zeta", "A.txt", "b.txt"]);
    }

    #[test]
    fn hides_hidden_entries_unless_requested() {
        let reader = Rc::new(FakeReader(Ok(vec![
            entry(".config", EntryKind::Directory, true),
            entry("notes", EntryKind::File, false),
        ])));
        let use_case = ListDirectory::new(reader);

        let visible = block_on(use_case.execute(&Location::Trash, ListingOptions::default()));
        assert_eq!(names(&visible.unwrap()), ["notes"]);

        let all =
            block_on(use_case.execute(&Location::Trash, ListingOptions { show_hidden: true }));
        assert_eq!(names(&all.unwrap()), [".config", "notes"]);
    }

    #[test]
    fn propagates_errors() {
        let use_case = ListDirectory::new(Rc::new(FakeReader(Err(ListingError::PermissionDenied))));
        let result = block_on(use_case.execute(&Location::Trash, ListingOptions::default()));
        assert_eq!(result, Err(ListingError::PermissionDenied));
    }
}
