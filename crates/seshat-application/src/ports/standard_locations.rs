use seshat_domain::entities::StandardLocation;

/// Resolves the user's standard locations (XDG user directories, Trash).
///
/// Implementations must not hardcode paths and must omit locations that are
/// not configured or do not exist.
pub trait StandardLocations {
    fn standard_locations(&self) -> Vec<StandardLocation>;
}
