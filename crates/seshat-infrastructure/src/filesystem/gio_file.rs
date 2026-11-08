use gio::prelude::*;
use seshat_domain::entities::Location;

const TRASH_URI: &str = "trash:///";

/// Maps a domain location to a GIO file. Trash locations go through GVfs.
pub(crate) fn gio_file(location: &Location) -> gio::File {
    match location {
        Location::Path(path) => gio::File::for_path(path),
        Location::Trash(rel) if rel.as_os_str().is_empty() => gio::File::for_uri(TRASH_URI),
        Location::Trash(rel) => gio::File::for_uri(TRASH_URI).resolve_relative_path(rel),
    }
}
