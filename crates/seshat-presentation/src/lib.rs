//! Seshat presentation layer (GTK4 + Relm4).
//!
//! Renders application state and forwards user intent to application use
//! cases. Never performs filesystem I/O directly.

mod actions;
mod app;
mod clipboard;
mod conflicts;
pub mod diagnostics;
mod dialogs;
mod files;
mod keys;
mod operations_panel;
mod panel_view;
mod pathbar;
mod sidebar;
pub mod theme;
mod workspace_view;

pub use app::AppInit;

use diagnostics::DisplayBackend;

/// Runs the main window until the application exits.
pub fn run(app_id: &str, init: AppInit) {
    // GTK must not parse our own CLI flags.
    relm4::RelmApp::new(app_id)
        .with_args(Vec::new())
        .run::<app::App>(init);
}

/// Initialises GTK and reports which display backend it connected to,
/// without opening a window.
pub fn check_backend() -> DisplayBackend {
    if gtk::init().is_err() {
        return DisplayBackend::Unavailable;
    }
    diagnostics::detect_backend()
}
