/// The kinds of file operation the manager runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    Copy,
    Move,
    Rename,
    /// Permanent deletion; cannot be undone.
    Delete,
    Trash,
    Restore,
    CreateDirectory,
    CreateFile,
}

impl OperationKind {
    /// Stable identifier, also used by the IPC protocol.
    pub fn id(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Rename => "rename",
            Self::Delete => "delete",
            Self::Trash => "trash",
            Self::Restore => "restore",
            Self::CreateDirectory => "create_directory",
            Self::CreateFile => "create_file",
        }
    }

    /// Present-progressive verb for progress displays.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Copy => "Copying",
            Self::Move => "Moving",
            Self::Rename => "Renaming",
            Self::Delete => "Deleting",
            Self::Trash => "Moving to Trash",
            Self::Restore => "Restoring",
            Self::CreateDirectory => "Creating folder",
            Self::CreateFile => "Creating file",
        }
    }
}
