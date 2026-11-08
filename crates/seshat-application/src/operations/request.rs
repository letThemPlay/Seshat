use std::path::{Component, Path, PathBuf};

use seshat_domain::entities::Location;
use seshat_domain::operation::OperationKind;
use thiserror::Error;

/// A request to perform a file operation. Validated before an operation is
/// created; IPC and UI requests go through the same validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationRequest {
    Copy {
        sources: Vec<PathBuf>,
        destination: PathBuf,
    },
    Move {
        sources: Vec<PathBuf>,
        destination: PathBuf,
    },
    Rename {
        path: PathBuf,
        new_name: String,
    },
    /// Permanent deletion of paths and/or Trash items.
    Delete {
        items: Vec<Location>,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
    /// Restores top-level Trash items (relative to the Trash root).
    Restore {
        items: Vec<PathBuf>,
    },
    CreateDirectory {
        parent: PathBuf,
        name: String,
    },
    CreateFile {
        parent: PathBuf,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RequestError {
    #[error("nothing to do: no items given")]
    NoItems,
    #[error("“{}” is not an absolute path", .0.display())]
    NotAbsolute(PathBuf),
    #[error("the root folder cannot be {0}")]
    Root(&'static str),
    #[error("cannot {verb} “{}” into itself", item.display())]
    IntoItself { verb: &'static str, item: PathBuf },
    #[error("invalid name: {0}")]
    InvalidName(&'static str),
    #[error("only items at the top level of the Trash can be restored")]
    NotTopLevelTrashItem,
}

/// Longest file name most Linux filesystems accept, in bytes.
const MAX_NAME_BYTES: usize = 255;

/// Validates a single file name typed by the user.
pub fn validate_name(name: &str) -> Result<(), RequestError> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(RequestError::InvalidName("the name is empty"));
    }
    if name == "." || name == ".." {
        return Err(RequestError::InvalidName("“.” and “..” are reserved"));
    }
    if name.contains('/') {
        return Err(RequestError::InvalidName("names cannot contain “/”"));
    }
    if name.contains('\0') {
        return Err(RequestError::InvalidName("names cannot contain NUL"));
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(RequestError::InvalidName("the name is too long"));
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<(), RequestError> {
    if path.is_absolute() && !path.components().any(|c| c == Component::ParentDir) {
        Ok(())
    } else {
        Err(RequestError::NotAbsolute(path.to_path_buf()))
    }
}

fn not_root(path: &Path, verb: &'static str) -> Result<(), RequestError> {
    if path.parent().is_none() {
        Err(RequestError::Root(verb))
    } else {
        Ok(())
    }
}

fn top_level_trash_item(item: &Path) -> Result<(), RequestError> {
    let mut components = item.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(RequestError::NotTopLevelTrashItem),
    }
}

impl OperationRequest {
    pub fn kind(&self) -> OperationKind {
        match self {
            Self::Copy { .. } => OperationKind::Copy,
            Self::Move { .. } => OperationKind::Move,
            Self::Rename { .. } => OperationKind::Rename,
            Self::Delete { .. } => OperationKind::Delete,
            Self::Trash { .. } => OperationKind::Trash,
            Self::Restore { .. } => OperationKind::Restore,
            Self::CreateDirectory { .. } => OperationKind::CreateDirectory,
            Self::CreateFile { .. } => OperationKind::CreateFile,
        }
    }

    /// The items the operation acts on, for display and IPC.
    pub fn sources(&self) -> Vec<Location> {
        let paths = |ps: &[PathBuf]| ps.iter().cloned().map(Location::Path).collect();
        match self {
            Self::Copy { sources, .. } | Self::Move { sources, .. } => paths(sources),
            Self::Rename { path, .. } => vec![Location::Path(path.clone())],
            Self::Delete { items } => items.clone(),
            Self::Trash { paths: ps } => paths(ps),
            Self::Restore { items } => items.iter().cloned().map(Location::Trash).collect(),
            Self::CreateDirectory { parent, name } | Self::CreateFile { parent, name } => {
                vec![Location::Path(parent.join(name))]
            }
        }
    }

    pub fn destination(&self) -> Option<PathBuf> {
        match self {
            Self::Copy { destination, .. } | Self::Move { destination, .. } => {
                Some(destination.clone())
            }
            Self::Rename { path, new_name } => path.parent().map(|p| p.join(new_name)),
            Self::CreateDirectory { parent, .. } | Self::CreateFile { parent, .. } => {
                Some(parent.clone())
            }
            Self::Delete { .. } | Self::Trash { .. } | Self::Restore { .. } => None,
        }
    }

    pub fn validate(&self) -> Result<(), RequestError> {
        match self {
            Self::Copy {
                sources,
                destination,
            }
            | Self::Move {
                sources,
                destination,
            } => {
                let verb = if matches!(self, Self::Copy { .. }) {
                    "copy"
                } else {
                    "move"
                };
                if sources.is_empty() {
                    return Err(RequestError::NoItems);
                }
                absolute(destination)?;
                for source in sources {
                    absolute(source)?;
                    not_root(source, if verb == "copy" { "copied" } else { "moved" })?;
                    if destination.starts_with(source) {
                        return Err(RequestError::IntoItself {
                            verb,
                            item: source.clone(),
                        });
                    }
                }
                Ok(())
            }
            Self::Rename { path, new_name } => {
                absolute(path)?;
                not_root(path, "renamed")?;
                validate_name(new_name)
            }
            Self::Delete { items } => {
                if items.is_empty() {
                    return Err(RequestError::NoItems);
                }
                for item in items {
                    match item {
                        Location::Path(path) => {
                            absolute(path)?;
                            not_root(path, "deleted")?;
                        }
                        Location::Trash(rel) => top_level_trash_item(rel)?,
                    }
                }
                Ok(())
            }
            Self::Trash { paths } => {
                if paths.is_empty() {
                    return Err(RequestError::NoItems);
                }
                for path in paths {
                    absolute(path)?;
                    not_root(path, "moved to the Trash")?;
                }
                Ok(())
            }
            Self::Restore { items } => {
                if items.is_empty() {
                    return Err(RequestError::NoItems);
                }
                items.iter().try_for_each(|i| top_level_trash_item(i))
            }
            Self::CreateDirectory { parent, name } | Self::CreateFile { parent, name } => {
                absolute(parent)?;
                validate_name(name)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(sources: &[&str], dest: &str) -> OperationRequest {
        OperationRequest::Copy {
            sources: sources.iter().map(PathBuf::from).collect(),
            destination: dest.into(),
        }
    }

    #[test]
    fn names() {
        assert!(validate_name("report.pdf").is_ok());
        assert!(validate_name(" leading space").is_ok());
        for bad in ["", "   ", ".", "..", "a/b", "a\0b"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_name(&"x".repeat(256)).is_err());
        assert!(validate_name(&"é".repeat(127)).is_ok()); // 254 bytes
    }

    #[test]
    fn copy_validation() {
        assert!(copy(&["/home/u/a"], "/home/u/b").validate().is_ok());
        assert!(
            copy(&["/home/u/a"], "/home/u").validate().is_ok(),
            "same folder → duplicate"
        );
        assert_eq!(copy(&[], "/x").validate(), Err(RequestError::NoItems));
        assert!(matches!(
            copy(&["/home/u/a"], "/home/u/a/sub").validate(),
            Err(RequestError::IntoItself { .. })
        ));
        assert!(matches!(
            copy(&["/home/u/a"], "/home/u/a").validate(),
            Err(RequestError::IntoItself { .. })
        ));
        assert!(
            copy(&["/home/u/a"], "/home/u/ab").validate().is_ok(),
            "component-wise"
        );
        assert!(matches!(
            copy(&["rel"], "/x").validate(),
            Err(RequestError::NotAbsolute(_))
        ));
        assert!(matches!(
            copy(&["/a/../b"], "/x").validate(),
            Err(RequestError::NotAbsolute(_))
        ));
        assert!(matches!(
            copy(&["/"], "/x").validate(),
            Err(RequestError::Root(_))
        ));
    }

    #[test]
    fn trash_and_restore_validation() {
        let restore = |s: &str| OperationRequest::Restore {
            items: vec![s.into()],
        };
        assert!(restore("file.txt").validate().is_ok());
        assert_eq!(
            restore("dir/file.txt").validate(),
            Err(RequestError::NotTopLevelTrashItem)
        );
        assert_eq!(
            restore("../x").validate(),
            Err(RequestError::NotTopLevelTrashItem)
        );
        let delete = OperationRequest::Delete {
            items: vec![Location::Trash("a".into()), Location::Path("/tmp/x".into())],
        };
        assert!(delete.validate().is_ok());
        let trash_root = OperationRequest::Trash {
            paths: vec!["/".into()],
        };
        assert!(matches!(trash_root.validate(), Err(RequestError::Root(_))));
    }

    #[test]
    fn rename_and_create() {
        let rename = OperationRequest::Rename {
            path: "/d/a".into(),
            new_name: "b".into(),
        };
        assert!(rename.validate().is_ok());
        assert_eq!(rename.destination(), Some(PathBuf::from("/d/b")));
        let bad = OperationRequest::CreateDirectory {
            parent: "/d".into(),
            name: "x/y".into(),
        };
        assert!(bad.validate().is_err());
    }
}
