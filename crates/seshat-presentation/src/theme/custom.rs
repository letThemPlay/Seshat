//! User-defined themes: TOML files that extend a built-in theme.
//!
//! ```toml
//! name = "Nord"
//! extends = "dark"          # "light" (default) or "dark"
//!
//! [colors]
//! surface = "#2e3440"
//! accent = "#88c0d0"
//!
//! [geometry]
//! border_radius = 6
//!
//! [typography]
//! font_family = "Inter"
//! font_size = 11
//! ```
//!
//! Invalid or unknown values are reported as warnings and fall back to the
//! base theme, so a typo never breaks the UI. The selected navigation
//! radius and the shared surface token are not configurable.

use toml::{Table, Value};

use super::{Color, Theme, dark, light};

/// A parsed custom theme with any problems found while validating it.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomTheme {
    pub id: String,
    pub theme: Theme,
    pub warnings: Vec<String>,
}

/// Minimum contrast ratio for body text (WCAG 2.1 AA).
const MIN_TEXT_CONTRAST: f64 = 4.5;

pub fn parse_custom_theme(id: &str, source: &str) -> Result<CustomTheme, String> {
    let table: Table = source
        .parse()
        .map_err(|e: toml::de::Error| e.message().to_owned())?;
    let mut warnings = Vec::new();

    let mut theme = match table.get("extends") {
        None => light(),
        Some(Value::String(s)) if s == "light" => light(),
        Some(Value::String(s)) if s == "dark" => dark(),
        Some(other) => {
            warnings.push(format!(
                "extends: expected \"light\" or \"dark\", found {other}"
            ));
            light()
        }
    };
    theme.name = match table.get("name") {
        Some(Value::String(name)) if !name.trim().is_empty() => name.trim().to_owned(),
        Some(other) => {
            warnings.push(format!("name: expected text, found {other}"));
            id.to_owned()
        }
        None => id.to_owned(),
    };

    for (key, value) in &table {
        match key.as_str() {
            "name" | "extends" => {}
            "colors" => with_table(key, value, &mut warnings, |t, w| {
                apply_colors(&mut theme, t, w)
            }),
            "geometry" => with_table(key, value, &mut warnings, |t, w| {
                apply_geometry(&mut theme, t, w)
            }),
            "typography" => {
                with_table(key, value, &mut warnings, |t, w| {
                    apply_typography(&mut theme, t, w)
                });
            }
            other => warnings.push(format!("unknown setting “{other}”")),
        }
    }

    check_contrast(&theme, &mut warnings);
    Ok(CustomTheme {
        id: id.to_owned(),
        theme,
        warnings,
    })
}

fn with_table(
    key: &str,
    value: &Value,
    warnings: &mut Vec<String>,
    apply: impl FnOnce(&Table, &mut Vec<String>),
) {
    match value {
        Value::Table(table) => apply(table, warnings),
        other => warnings.push(format!("{key}: expected a [{key}] section, found {other}")),
    }
}

fn apply_colors(theme: &mut Theme, table: &Table, warnings: &mut Vec<String>) {
    let c = &mut theme.colors;
    for (key, value) in table {
        let slot = match key.as_str() {
            "app_background" => &mut c.app_background,
            "surface" => &mut c.surface,
            "navigation_surface" => &mut c.navigation_surface,
            "text_primary" => &mut c.text_primary,
            "text_secondary" => &mut c.text_secondary,
            "text_muted" => &mut c.text_muted,
            "accent" => &mut c.accent,
            "border" => &mut c.border,
            "hover" => &mut c.hover,
            "selected" => &mut c.selected,
            "focus" => &mut c.focus,
            "error" => &mut c.error,
            "warning" => &mut c.warning,
            "success" => &mut c.success,
            other => {
                warnings.push(format!("colors: unknown colour “{other}”"));
                continue;
            }
        };
        match value.as_str().map(str::parse::<Color>) {
            Some(Ok(color)) => *slot = color,
            Some(Err(err)) => warnings.push(format!("colors.{key}: {err}")),
            None => warnings.push(format!("colors.{key}: expected \"#rrggbb\", found {value}")),
        }
    }
}

fn apply_geometry(theme: &mut Theme, table: &Table, warnings: &mut Vec<String>) {
    let g = &mut theme.geometry;
    for (key, value) in table {
        let (slot, range) = match key.as_str() {
            "border_radius" => (&mut g.border_radius, 0..=24),
            "padding" => (&mut g.padding, 0..=32),
            "spacing" => (&mut g.spacing, 0..=32),
            "row_height" => (&mut g.row_height, 20..=96),
            "toolbar_height" => (&mut g.toolbar_height, 32..=96),
            "icon_size" => (&mut g.icon_size, 12..=64),
            other => {
                warnings.push(format!("geometry: unknown setting “{other}”"));
                continue;
            }
        };
        match integer_in(value, &range) {
            Some(n) => *slot = n,
            None => warnings.push(format!(
                "geometry.{key}: expected a whole number from {} to {}, found {value}",
                range.start(),
                range.end()
            )),
        }
    }
}

fn apply_typography(theme: &mut Theme, table: &Table, warnings: &mut Vec<String>) {
    let t = &mut theme.typography;
    for (key, value) in table {
        match key.as_str() {
            "font_family" => match value.as_str().map(str::trim) {
                Some(family)
                    if !family.is_empty()
                        && family.len() <= 100
                        && !family
                            .chars()
                            .any(|ch| ch.is_control() || ch == '"' || ch == '\\') =>
                {
                    t.font_family = family.to_owned();
                }
                _ => warnings.push(format!("typography.font_family: invalid font name {value}")),
            },
            "font_size" => match integer_in(value, &(6..=36)) {
                Some(n) => t.font_size = n,
                None => warnings.push(format!(
                    "typography.font_size: expected 6 to 36 (points), found {value}"
                )),
            },
            "font_weight" => match integer_in(value, &(100..=900)).filter(|n| n % 100 == 0) {
                Some(n) => t.font_weight = n,
                None => warnings.push(format!(
                    "typography.font_weight: expected 100, 200, … 900, found {value}"
                )),
            },
            "line_height" => match value
                .as_float()
                .or_else(|| value.as_integer().map(|i| i as f64))
            {
                Some(lh) if (0.8..=3.0).contains(&lh) => t.line_height = Some(lh as f32),
                _ => warnings.push(format!(
                    "typography.line_height: expected 0.8 to 3.0, found {value}"
                )),
            },
            other => warnings.push(format!("typography: unknown setting “{other}”")),
        }
    }
}

fn integer_in(value: &Value, range: &std::ops::RangeInclusive<u32>) -> Option<u32> {
    value
        .as_integer()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| range.contains(n))
}

fn check_contrast(theme: &Theme, warnings: &mut Vec<String>) {
    let c = &theme.colors;
    for (name, background) in [
        ("surface", c.surface),
        ("navigation_surface", c.navigation_surface),
        ("app_background", c.app_background),
    ] {
        let ratio = contrast_ratio(c.text_primary, background);
        if ratio < MIN_TEXT_CONTRAST {
            warnings.push(format!(
                "low contrast: text_primary on {name} is {ratio:.1}:1 (at least {MIN_TEXT_CONTRAST}:1 recommended)"
            ));
        }
    }
}

/// WCAG 2.1 contrast ratio. Alpha is ignored (colours treated as opaque).
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn luminance(c: Color) -> f64 {
    let channel = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::render_css;

    fn parse(src: &str) -> CustomTheme {
        parse_custom_theme("test", src).expect("valid TOML")
    }

    #[test]
    fn empty_file_is_light_named_after_id() {
        let t = parse("");
        assert_eq!(t.theme.name, "test");
        assert_eq!(t.theme.colors, light().colors);
        assert!(t.warnings.is_empty());
    }

    #[test]
    fn extends_dark_and_overrides_values() {
        let t = parse(
            r##"
            name = "Nord"
            extends = "dark"
            [colors]
            surface = "#2e3440"
            [geometry]
            border_radius = 4
            [typography]
            font_family = "Inter"
            line_height = 1.4
            "##,
        );
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.theme.name, "Nord");
        assert!(t.theme.dark);
        assert_eq!(t.theme.colors.surface, Color::rgb(0x2e, 0x34, 0x40));
        assert_eq!(t.theme.colors.accent, dark().colors.accent);
        assert_eq!(t.theme.geometry.border_radius, 4);
        assert_eq!(t.theme.typography.font_family, "Inter");
        assert_eq!(t.theme.typography.line_height, Some(1.4));
    }

    #[test]
    fn invalid_values_fall_back_with_warnings() {
        let t = parse(
            r##"
            extends = "neon"
            mystery = 1
            [colors]
            surface = "white"
            accent = 42
            sparkle = "#ffffff"
            [geometry]
            border_radius = 100
            padding = -1
            [typography]
            font_size = 2
            font_weight = 450
            font_family = "Evil\"; } * { color: red"
            "##,
        );
        let base = light();
        assert_eq!(t.theme.colors.surface, base.colors.surface);
        assert_eq!(t.theme.colors.accent, base.colors.accent);
        assert_eq!(t.theme.geometry, base.geometry);
        assert_eq!(t.theme.typography, base.typography);
        assert_eq!(t.warnings.len(), 10, "{:#?}", t.warnings);
    }

    #[test]
    fn selected_nav_radius_cannot_be_overridden() {
        let t = parse("[geometry]\nnavigation_selected_radius = 0\n");
        assert_eq!(t.warnings.len(), 1);
        let css = render_css(&t.theme);
        assert!(css.contains("border-radius: 12px;"));
    }

    #[test]
    fn surface_override_applies_to_content_and_selected_nav() {
        let t = parse("[colors]\nsurface = \"#123456\"\ntext_primary = \"#ffffff\"\n");
        let css = render_css(&t.theme);
        assert_eq!(css.matches("#123456").count(), 1, "defined once");
        assert!(css.contains("@define-color seshat_surface #123456;"));
    }

    #[test]
    fn low_contrast_is_warned_not_rejected() {
        let t = parse("[colors]\ntext_primary = \"#eeeeee\"\n");
        assert_eq!(t.theme.colors.text_primary, Color::rgb(0xee, 0xee, 0xee));
        assert!(t.warnings.iter().any(|w| w.starts_with("low contrast")));
    }

    #[test]
    fn documented_example_theme_is_valid() {
        let source = include_str!("../../../../docs/themes/nord.toml");
        let t = parse_custom_theme("nord", source).expect("example parses");
        assert!(t.warnings.is_empty(), "{:#?}", t.warnings);
        assert!(t.theme.dark);
    }

    #[test]
    fn catppuccin_mocha_example_theme_is_valid() {
        let source = include_str!("../../../../docs/themes/catppuccin-mocha.toml");
        let t = parse_custom_theme("catppuccin-mocha", source).expect("example parses");
        assert!(t.warnings.is_empty(), "{:#?}", t.warnings);
        assert!(t.theme.dark);
        assert_eq!(t.theme.name, "Catppuccin Mocha");
        assert_eq!(t.theme.colors.accent, Color::rgb(0xcb, 0xa6, 0xf7));
    }

    #[test]
    fn syntax_errors_reject_the_theme() {
        assert!(parse_custom_theme("bad", "name = ").is_err());
    }

    #[test]
    fn contrast_ratio_known_values() {
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn builtin_themes_pass_the_contrast_check() {
        for theme in [light(), dark()] {
            let mut warnings = Vec::new();
            check_contrast(&theme, &mut warnings);
            assert!(warnings.is_empty(), "{}: {warnings:?}", theme.name);
        }
    }
}
