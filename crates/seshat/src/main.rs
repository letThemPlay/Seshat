//! Composition root: wires infrastructure implementations into the
//! application use cases and hands them to the presentation layer.

use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use seshat_application::operations::OperationManager;
use seshat_application::ports::{StandardLocations, ThemeRepository};
use seshat_application::use_cases::ListDirectory;
use seshat_domain::entities::{Location, StandardLocationKind};
use seshat_infrastructure::filesystem::{
    GioDeviceMonitor, GioDirectoryReader, GioDirectoryWatcher, GioLauncher, LocalFileSystem,
    XdgStandardLocations,
};
use seshat_infrastructure::ipc::{IpcConfig, IpcService};
use seshat_infrastructure::persistence::{
    FileThemeRepository, TomlPreferencesStore, TomlWorkspaceStore,
};
use seshat_infrastructure::search::{LocalTreeWalker, NucleoMatcher};
use seshat_presentation::AppInit;
use tracing_subscriber::EnvFilter;

/// Application ID (reverse-DNS). The desktop entry, icon, AppStream id and
/// the D-Bus service name ([`seshat_infrastructure::ipc`]) all derive from it.
const APP_ID: &str = "dev.kdmorris.Seshat";

/// File operations running at once; more are queued.
const MAX_CONCURRENT_OPERATIONS: usize = 3;

const USAGE: &str = "\
Usage: seshat [OPTIONS]

Options:
  --theme-file FILE   Start with the theme in FILE (a theme file, see
                      docs/development/themes.md). It is listed in the theme
                      menu and wins over the saved theme, but is never saved
  --check-theme FILE  Check the theme in FILE without opening a window: print
                      its problems and exit with status 0 only if it has none
  --check-backend     Connect to the display, print the detected backend and
                      exit with status 0 only if it is native Wayland
  -h, --help          Show this help";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run { theme_file: Option<PathBuf> },
    CheckTheme(PathBuf),
    CheckBackend,
    Help,
}

/// Parses the arguments (without the program name). `--theme-file` may come
/// before any command, because the Home Manager wrapper always adds it.
fn parse_args(args: &[String]) -> Option<Command> {
    let mut theme_file = None;
    let mut rest: Vec<&str> = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--theme-file" {
            theme_file = Some(PathBuf::from(args.next()?));
        } else {
            rest.push(arg);
        }
    }
    Some(match rest.as_slice() {
        [] => Command::Run { theme_file },
        ["--check-theme", file] => Command::CheckTheme(PathBuf::from(file)),
        ["--check-backend"] => Command::CheckBackend,
        ["-h" | "--help"] => Command::Help,
        _ => return None,
    })
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Some(Command::Run { theme_file }) => run_app(theme_file),
        Some(Command::CheckTheme(file)) => check_theme(file),
        Some(Command::CheckBackend) => check_backend(),
        Some(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_app(theme_file: Option<PathBuf>) -> ExitCode {
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
        themes: Rc::new(match theme_file {
            Some(file) => FileThemeRepository::default_location().with_provided(file),
            None => FileThemeRepository::default_location(),
        }),
        workspace_store: Rc::new(TomlWorkspaceStore::default_location()),
        operations: operations.clone(),
        search_walker: Arc::new(LocalTreeWalker::new()),
        search_matcher: Arc::new(NucleoMatcher::new()),
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

/// Loads `file` exactly as `--theme-file` would, without GTK, and reports
/// every problem. Used by the Nix checks on generated themes.
fn check_theme(file: PathBuf) -> ExitCode {
    let repository = FileThemeRepository::new(PathBuf::new()).with_provided(file);
    let Some(source) = repository.load_provided() else {
        return ExitCode::FAILURE;
    };
    let path = source.path.display().to_string();
    let parsed = source
        .contents
        .and_then(|text| seshat_presentation::theme::parse_custom_theme(&source.id, &text));
    match parsed {
        Ok(theme) if theme.warnings.is_empty() => {
            println!("{path}: theme “{}” has no problems", theme.theme.name);
            ExitCode::SUCCESS
        }
        Ok(theme) => {
            for warning in &theme.warnings {
                eprintln!("{path}: {warning}");
            }
            eprintln!(
                "{path}: theme “{}” has {} problem(s)",
                theme.theme.name,
                theme.warnings.len()
            );
            ExitCode::FAILURE
        }
        Err(message) => {
            eprintln!("{path}: not loaded: {message}");
            ExitCode::FAILURE
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Option<Command> {
        parse_args(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn no_arguments_run_without_a_theme_file() {
        assert_eq!(parse(&[]), Some(Command::Run { theme_file: None }));
    }

    #[test]
    fn a_theme_file_is_passed_to_the_app() {
        assert_eq!(
            parse(&["--theme-file", "/nix/store/x-theme.toml"]),
            Some(Command::Run {
                theme_file: Some("/nix/store/x-theme.toml".into())
            })
        );
        assert_eq!(parse(&["--theme-file"]), None, "the file is required");
    }

    /// The Home Manager wrapper always adds `--theme-file`, so every other
    /// command must still work after it.
    #[test]
    fn other_commands_work_after_the_wrappers_theme_file() {
        let wrapped = |args: &[&str]| {
            let mut all = vec!["--theme-file", "/t.toml"];
            all.extend_from_slice(args);
            parse(&all)
        };
        assert_eq!(wrapped(&["--check-backend"]), Some(Command::CheckBackend));
        assert_eq!(wrapped(&["--help"]), Some(Command::Help));
        assert_eq!(
            wrapped(&["--check-theme", "/other.toml"]),
            Some(Command::CheckTheme("/other.toml".into()))
        );
    }

    #[test]
    fn unknown_arguments_are_rejected() {
        assert_eq!(parse(&["--check-theme"]), None);
        assert_eq!(parse(&["--frobnicate"]), None);
        assert_eq!(parse(&["/home"]), None);
    }
}
