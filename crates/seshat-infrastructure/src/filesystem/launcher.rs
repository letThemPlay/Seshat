use gio::prelude::*;
use seshat_application::LocalBoxFuture;
use seshat_application::ports::Launcher;
use seshat_domain::entities::Location;

use super::gio_file::gio_file;

/// Opens files with the default handler via GIO. Inside a sandbox GIO uses
/// the OpenURI desktop portal automatically.
#[derive(Debug, Default, Clone, Copy)]
pub struct GioLauncher;

impl GioLauncher {
    pub fn new() -> Self {
        Self
    }
}

impl Launcher for GioLauncher {
    fn open<'a>(&'a self, location: &'a Location) -> LocalBoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let uri = gio_file(location).uri();
            gio::AppInfo::launch_default_for_uri_future(&uri, gio::AppLaunchContext::NONE)
                .await
                .map_err(|err| {
                    tracing::warn!(error = %err, "failed to open file");
                    err.message().to_owned()
                })
        })
    }
}
