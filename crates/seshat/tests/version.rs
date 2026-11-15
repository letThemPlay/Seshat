//! `seshat --version` prints the workspace version that every release
//! records (docs/development/releasing.md), without a display.

use std::process::Command;

#[test]
fn prints_the_workspace_version() {
    for flag in ["--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_seshat"))
            .arg(flag)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .output()
            .expect("seshat runs");
        assert!(output.status.success(), "{flag}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("seshat {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
    }
}
