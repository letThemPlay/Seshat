use seshat_domain::workspace::Workspace;

/// Version of the persisted workspace format. Bump it when the format
/// changes incompatibly and add a migration in the store implementation.
pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;

/// Saves and restores the workspace between sessions.
pub trait WorkspaceStore {
    /// The saved workspace, or `None` if there is none or it cannot be
    /// used (unreadable, invalid, or from a newer version). Never fails:
    /// a broken file must not stop the application from starting.
    fn load(&self) -> Option<Workspace>;

    /// Saves the workspace. Implementations may finish writing in the
    /// background; [`Self::flush`] waits for that.
    fn save(&self, workspace: &Workspace) -> Result<(), String>;

    /// Waits until every save has reached the disk (before quitting).
    fn flush(&self) {}
}
