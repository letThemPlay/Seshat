use seshat_domain::entities::Location;

use crate::LocalBoxFuture;

/// Opens files with the user's default application.
pub trait Launcher {
    fn open<'a>(&'a self, location: &'a Location) -> LocalBoxFuture<'a, Result<(), String>>;
}
