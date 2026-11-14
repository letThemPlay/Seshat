//! `seshat --check-theme`: the Nix checks rely on its exit status. It must
//! work without a display.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn check(file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_seshat"))
        .arg("--check-theme")
        .arg(file)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .output()
        .expect("seshat runs")
}

fn theme(name: &str, contents: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn the_example_themes_have_no_problems() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/themes");
    for name in ["nord.toml", "catppuccin-mocha.toml"] {
        let output = check(&docs.join(name));
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn a_theme_with_problems_fails_and_names_them() {
    let file = theme(
        "check-theme-warnings.toml",
        "[colors]\nsurface = \"white\"\n[geometry]\nborder_radius = 100\n",
    );
    let output = check(&file);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("colors.surface"), "{stderr}");
    assert!(stderr.contains("geometry.border_radius"), "{stderr}");
}

#[test]
fn an_unloadable_theme_fails() {
    let broken = theme("check-theme-broken.toml", "name = ");
    assert!(!check(&broken).status.success());
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("check-theme-missing.toml");
    assert!(!check(&missing).status.success());
}
