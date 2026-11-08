//! Composition root: wires infrastructure implementations into the
//! application use cases and hands them to the presentation layer.

use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use seshat_application::operations::OperationManager;
use seshat_application::ports::StandardLocations;
use seshat_application::use_cases::ListDirectory;
use seshat_domain::entities::{Location, StandardLocationKind};
use seshat_infrastructure::filesystem::{
    GioDeviceMonitor, GioDirectoryReader, GioDirectoryWatcher, GioLauncher, LocalFileSystem,
    XdgStandardLocations,
};
use seshat_infrastructure::ipc::{IpcConfig, IpcService};
use seshat_infrastructure::persistence::{FileThemeRepository, TomlPreferencesStore};
use seshat_presentation::AppInit;
use tracing_subscriber::EnvFilter;

/// Application ID (reverse-DNS). The desktop entry, icon, AppStream id and
/// the D-Bus service name (`seshat-infrastructure::ipc`) all derive from it.
const APP_ID: &str = "dev.kdmorris.Seshat";

/// File operations running at once; more are queued.
const MAX_CONCURRENT_OPERATIONS: usize = 3;

const USAGE: &str = "\
Usage: seshat [OPTIONS]

Options:
  --check-backend  Connect to the display, print the detected backend and
                   exit with status 0 only if it is native Wayland
  -h, --help       Show this help";

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => run_app(),
        ["--check-backend"] => check_backend(),
        ["-h" | "--help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_app() -> ExitCode {
    let locations = XdgStandardLocations::new().standard_locations();
    let home = locations
        .iter()
        .find_map(|l| match (&l.kind, &l.location) {
            (StandardLocationKind::Home, Location::Path(path)) => Some(path.clone()),
            _ => None,
        })
        .expect("standard locations always include Home");

    let operations =
        OperationManager::new(Arc::new(LocalFileSystem::new()), MAX_CONCURRENT_OPERATIONS);

    let init = AppInit {
        home,
        list_directory: ListDirectory::new(Rc::new(GioDirectoryReader::new())),
        locations,
        directory_watcher: Rc::new(GioDirectoryWatcher::new()),
        device_monitor: Rc::new(GioDeviceMonitor::new()),
        launcher: Rc::new(GioLauncher::new()),
        preferences: Rc::new(TomlPreferencesStore::default_location()),
        themes: Rc::new(FileThemeRepository::default_location()),
        operations: operations.clone(),
    };

    // Other applications observe operations over D-Bus. Seshat works
    // without it (no session bus, or another instance owns the name).
    let _ipc = match IpcService::start(operations, IpcConfig::default()) {
        Ok(service) => Some(service),
        Err(err) => {
            tracing::warn!(error = %err, "D-Bus service unavailable; operations are not published");
            None
        }
    };
    seshat_presentation::run(APP_ID, init);
    ExitCode::SUCCESS
}

fn check_backend() -> ExitCode {
    let backend = seshat_presentation::check_backend();
    seshat_presentation::diagnostics::log_backend(&backend);
    println!("display backend: {backend}");
    if backend.is_native_wayland() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
