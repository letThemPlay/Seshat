//! Runtime verification of the display backend.
//!
//! Environment variables only describe the session; they don't prove which
//! backend GTK actually connected to. The authoritative check is the concrete
//! type of the default `GdkDisplay`.

use std::fmt;

use gtk::gdk;
use gtk::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayBackend {
    /// Native Wayland client. Holds the compositor socket name.
    Wayland { display_name: String },
    /// X11 connection. Inside a Wayland session this means XWayland.
    X11 {
        display_name: String,
        inside_wayland_session: bool,
    },
    /// Any other GDK backend (e.g. broadway).
    Other { type_name: String },
    /// GTK could not open a display at all.
    Unavailable,
}

impl DisplayBackend {
    pub fn is_native_wayland(&self) -> bool {
        matches!(self, Self::Wayland { .. })
    }
}

impl fmt::Display for DisplayBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland { display_name } => write!(f, "wayland (native, {display_name})"),
            Self::X11 {
                display_name,
                inside_wayland_session: true,
            } => write!(f, "x11 via XWayland ({display_name})"),
            Self::X11 { display_name, .. } => write!(f, "x11 ({display_name})"),
            Self::Other { type_name } => write!(f, "other ({type_name})"),
            Self::Unavailable => f.write_str("unavailable"),
        }
    }
}

/// Session environment, recorded for diagnostics only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnvironment {
    pub wayland_display: Option<String>,
    pub xdg_session_type: Option<String>,
    pub x11_display: Option<String>,
    pub gdk_backend: Option<String>,
}

impl SessionEnvironment {
    pub fn capture() -> Self {
        let var = |name| std::env::var(name).ok();
        Self {
            wayland_display: var("WAYLAND_DISPLAY"),
            xdg_session_type: var("XDG_SESSION_TYPE"),
            x11_display: var("DISPLAY"),
            gdk_backend: var("GDK_BACKEND"),
        }
    }

    fn is_wayland_session(&self) -> bool {
        self.wayland_display.is_some() || self.xdg_session_type.as_deref() == Some("wayland")
    }
}

/// Detects the backend of the default display. GTK must be initialised.
pub fn detect_backend() -> DisplayBackend {
    let Some(display) = gdk::Display::default() else {
        return DisplayBackend::Unavailable;
    };
    classify(&display, &SessionEnvironment::capture())
}

fn classify(display: &gdk::Display, env: &SessionEnvironment) -> DisplayBackend {
    let display_name = display.name().to_string();
    if display.is::<gdk4_wayland::WaylandDisplay>() {
        DisplayBackend::Wayland { display_name }
    } else if display.is::<gdk4_x11::X11Display>() {
        DisplayBackend::X11 {
            display_name,
            inside_wayland_session: env.is_wayland_session(),
        }
    } else {
        DisplayBackend::Other {
            type_name: display.type_().name().to_owned(),
        }
    }
}

/// Logs the detected backend together with the session environment.
pub fn log_backend(backend: &DisplayBackend) {
    let env = SessionEnvironment::capture();
    if backend.is_native_wayland() {
        tracing::info!(
            %backend,
            wayland_display = ?env.wayland_display,
            xdg_session_type = ?env.xdg_session_type,
            x11_display = ?env.x11_display,
            gdk_backend = ?env.gdk_backend,
            "display backend verified"
        );
    } else {
        tracing::error!(
            %backend,
            wayland_display = ?env.wayland_display,
            xdg_session_type = ?env.xdg_session_type,
            x11_display = ?env.x11_display,
            gdk_backend = ?env.gdk_backend,
            "not running as a native Wayland client"
        );
    }
}

/// Main-loop gaps longer than this are reported by the stall monitor.
const STALL_THRESHOLD: std::time::Duration = std::time::Duration::from_millis(100);
const STALL_TICK: std::time::Duration = std::time::Duration::from_millis(20);

/// With `SESHAT_STALL_MONITOR=1`, logs every main-loop stall longer than
/// `STALL_THRESHOLD`: a timer that should fire every `STALL_TICK`
/// reports how late it was. Performance runs use it to check the UI stays
/// responsive (input is handled on the same loop). Off by default, since
/// it costs a wakeup every tick.
pub fn start_stall_monitor() {
    if std::env::var_os("SESHAT_STALL_MONITOR").is_none_or(|v| v != "1") {
        return;
    }
    tracing::info!("main-loop stall monitor enabled");
    let last = std::cell::Cell::new(std::time::Instant::now());
    gtk::glib::timeout_add_local(STALL_TICK, move || {
        let now = std::time::Instant::now();
        let gap = now.duration_since(last.replace(now));
        if gap > STALL_THRESHOLD {
            tracing::info!(gap_ms = gap.as_millis() as u64, "main loop stalled");
        }
        gtk::glib::ControlFlow::Continue
    });
}
