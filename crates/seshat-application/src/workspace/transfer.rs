use std::path::PathBuf;

use seshat_domain::entities::Location;
use thiserror::Error;

use super::SelectionGroup;
use crate::operations::OperationRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    Copy,
    Move,
}

impl TransferKind {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransferError {
    #[error("nothing is selected")]
    NothingSelected,
    #[error("files can't be copied or moved into the Trash this way")]
    DestinationNotAFolder,
    #[error("items in the Trash can only be restored or deleted")]
    SourceInTrash,
}

/// A copy or move across panels and branches, planned from the selection
/// and an explicit destination. Every source is an absolute path captured
/// now, so what runs never depends on which panel is active later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPlan {
    pub kind: TransferKind,
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
    /// Source folders and how many items come from each, in order.
    pub origins: Vec<(Location, usize)>,
}

impl TransferPlan {
    /// Plans a transfer of the workspace selection (grouped by panel).
    pub fn new(
        kind: TransferKind,
        groups: &[SelectionGroup],
        destination: &Location,
    ) -> Result<Self, TransferError> {
        Self::from_groups(
            kind,
            groups.iter().map(|g| (&g.folder, g.items.as_slice())),
            destination,
        )
    }

    /// Plans a transfer of loose paths from outside the workspace (the
    /// clipboard, drag and drop), grouped by their folder so the plan can
    /// say where the items come from.
    pub fn from_paths(
        kind: TransferKind,
        paths: &[PathBuf],
        destination: &Location,
    ) -> Result<Self, TransferError> {
        let mut groups: Vec<(Location, Vec<Location>)> = Vec::new();
        for path in paths {
            let folder = Location::Path(path.parent().map(PathBuf::from).unwrap_or_default());
            let item = Location::Path(path.clone());
            match groups.iter_mut().find(|(f, _)| *f == folder) {
                Some((_, items)) => items.push(item),
                None => groups.push((folder, vec![item])),
            }
        }
        Self::from_groups(
            kind,
            groups.iter().map(|(f, items)| (f, items.as_slice())),
            destination,
        )
    }

    fn from_groups<'a>(
        kind: TransferKind,
        groups: impl Iterator<Item = (&'a Location, &'a [Location])>,
        destination: &Location,
    ) -> Result<Self, TransferError> {
        let Location::Path(destination) = destination else {
            return Err(TransferError::DestinationNotAFolder);
        };
        let mut sources = Vec::new();
        let mut origins = Vec::new();
        for (folder, items) in groups {
            for item in items {
                match item {
                    Location::Path(path) => sources.push(path.clone()),
                    Location::Trash(_) => return Err(TransferError::SourceInTrash),
                }
            }
            if !items.is_empty() {
                origins.push((folder.clone(), items.len()));
            }
        }
        if sources.is_empty() {
            return Err(TransferError::NothingSelected);
        }
        Ok(Self {
            kind,
            sources,
            destination: destination.clone(),
            origins,
        })
    }

    /// Whether the items come from more than one folder (D7: confirm first).
    pub fn spans_panels(&self) -> bool {
        self.origins.len() > 1
    }

    /// Moving items into the folder they are already in does nothing.
    pub fn is_noop(&self) -> bool {
        self.kind == TransferKind::Move
            && self
                .sources
                .iter()
                .all(|s| s.parent() == Some(self.destination.as_path()))
    }

    /// The request for the operation manager, which validates and runs it.
    pub fn request(&self) -> OperationRequest {
        let (sources, destination) = (self.sources.clone(), self.destination.clone());
        match self.kind {
            TransferKind::Copy => OperationRequest::Copy {
                sources,
                destination,
            },
            TransferKind::Move => OperationRequest::Move {
                sources,
                destination,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_domain::workspace::PanelId;

    fn group(panel: u64, folder: &str, items: &[&str]) -> SelectionGroup {
        SelectionGroup {
            panel: PanelId::from_raw(panel),
            folder: Location::Path(folder.into()),
            items: items
                .iter()
                .map(|i| Location::Path(format!("{folder}/{i}").into()))
                .collect(),
        }
    }

    #[test]
    fn plans_a_multi_panel_copy_with_absolute_sources() {
        let groups = [
            group(1, "/h/Documents", &["a", "b"]),
            group(4, "/h/Pictures", &["c"]),
        ];
        let plan = TransferPlan::new(
            TransferKind::Copy,
            &groups,
            &Location::Path("/h/Downloads".into()),
        )
        .unwrap();
        assert!(plan.spans_panels());
        assert_eq!(
            plan.request(),
            OperationRequest::Copy {
                sources: vec![
                    "/h/Documents/a".into(),
                    "/h/Documents/b".into(),
                    "/h/Pictures/c".into()
                ],
                destination: "/h/Downloads".into(),
            }
        );
    }

    #[test]
    fn rejects_trash_and_empty_selections() {
        let dest = Location::Path("/d".into());
        assert_eq!(
            TransferPlan::new(TransferKind::Move, &[], &dest),
            Err(TransferError::NothingSelected)
        );
        assert_eq!(
            TransferPlan::new(
                TransferKind::Move,
                &[group(1, "/a", &["x"])],
                &Location::trash_root()
            ),
            Err(TransferError::DestinationNotAFolder)
        );
        let trash = SelectionGroup {
            panel: PanelId::from_raw(1),
            folder: Location::trash_root(),
            items: vec![Location::Trash("x".into())],
        };
        assert_eq!(
            TransferPlan::new(TransferKind::Copy, &[trash], &dest),
            Err(TransferError::SourceInTrash)
        );
    }

    #[test]
    fn moving_into_the_same_folder_is_a_noop() {
        let plan = TransferPlan::new(
            TransferKind::Move,
            &[group(1, "/a", &["x", "y"])],
            &Location::Path("/a".into()),
        )
        .unwrap();
        assert!(plan.is_noop());
    }
}
