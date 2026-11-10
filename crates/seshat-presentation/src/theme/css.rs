use std::fmt::Write as _;

use super::{NAV_SELECTED_RADIUS_PX, Theme};

/// Renders a theme into the application stylesheet.
///
/// Every colour is emitted once as a named `@define-color`; rules only
/// reference those names. In particular, the main content background and the
/// selected navigation item background both reference `@seshat_surface`, so
/// they cannot drift apart.
pub fn render_css(theme: &Theme) -> String {
    let c = &theme.colors;
    let g = &theme.geometry;
    let t = &theme.typography;

    let mut css = String::new();
    for (name, color) in [
        ("app_background", c.app_background),
        ("surface", c.surface),
        ("navigation_surface", c.navigation_surface),
        ("text_primary", c.text_primary),
        ("text_secondary", c.text_secondary),
        ("text_muted", c.text_muted),
        ("accent", c.accent),
        ("border", c.border),
        ("hover", c.hover),
        ("selected", c.selected),
        ("focus", c.focus),
        ("error", c.error),
        ("warning", c.warning),
        ("success", c.success),
    ] {
        let _ = writeln!(css, "@define-color seshat_{name} {};", color.to_css());
    }

    let font_family = t.font_family.replace(['\\', '"'], "");
    let line_height = t
        .line_height
        .map(|lh| format!("  line-height: {lh};\n"))
        .unwrap_or_default();
    let nav_radius = NAV_SELECTED_RADIUS_PX;

    let _ = write!(
        css,
        r#"
window.seshat {{
  background-color: @seshat_app_background;
  color: @seshat_text_primary;
  font-family: "{font_family}";
  font-size: {font_size}pt;
  font-weight: {font_weight};
{line_height}}}

.seshat-toolbar {{
  min-height: {toolbar_height}px;
  padding: 0 {padding}px;
  background-color: @seshat_app_background;
}}

.seshat-toolbar button,
.seshat-toolbar label {{
  color: @seshat_text_primary;
}}

.seshat-toolbar button {{
  border-radius: {radius}px;
  background-color: transparent;
}}

.seshat-toolbar button:hover,
.seshat-toolbar button:checked {{
  background-color: @seshat_hover;
}}

.seshat-path {{
  color: @seshat_text_secondary;
}}

.seshat-sidebar,
.seshat-sidebar list {{
  background-color: @seshat_navigation_surface;
}}

.seshat-sidebar row {{
  min-height: {row_height}px;
  margin: 2px {padding}px;
  padding: 0 {padding}px;
  border-radius: {radius}px;
  color: @seshat_text_secondary;
  background-color: transparent;
}}

.seshat-sidebar row:hover {{
  background-color: @seshat_hover;
}}

.seshat-sidebar row:selected,
.seshat-sidebar row:selected:hover {{
  background-color: @seshat_surface;
  border-radius: {nav_radius}px;
  color: @seshat_text_primary;
  box-shadow: inset 0 0 0 1px @seshat_border;
}}

.seshat-sidebar row:selected image {{
  color: @seshat_accent;
}}

.seshat-sidebar row image {{
  -gtk-icon-size: {icon_size}px;
}}

.seshat-sidebar row.seshat-sidebar-heading,
.seshat-sidebar row.seshat-sidebar-heading:hover {{
  min-height: 0;
  margin-top: {spacing}px;
  background-color: transparent;
  color: @seshat_text_muted;
  font-size: smaller;
  font-weight: bold;
}}

.seshat-sidebar .seshat-eject {{
  min-height: 0;
  min-width: 0;
  padding: 2px;
  color: @seshat_text_muted;
}}

.seshat-pathbar button {{
  padding: 2px 6px;
  border-radius: {radius}px;
  background-color: transparent;
}}

.seshat-pathbar button label {{
  color: @seshat_text_secondary;
}}

.seshat-pathbar button.current label {{
  color: @seshat_text_primary;
  font-weight: bold;
}}

.seshat-pathbar button:hover {{
  background-color: @seshat_hover;
}}

.seshat-pathbar entry {{
  min-width: 360px;
  border-radius: {radius}px;
  background-color: @seshat_surface;
  color: @seshat_text_primary;
  box-shadow: inset 0 0 0 1px @seshat_border;
}}

.seshat-content,
.seshat-content scrolledwindow,
.seshat-content listview {{
  background-color: @seshat_surface;
}}

.seshat-content listview row {{
  color: @seshat_text_primary;
  min-height: {row_height}px;
  padding: 0 {padding}px;
  border-radius: {radius}px;
}}

.seshat-content listview row:hover {{
  background-color: @seshat_hover;
}}

.seshat-content listview row:selected {{
  background-color: @seshat_selected;
}}

.seshat-content gridview {{
  background-color: @seshat_surface;
  padding: {padding}px;
}}

.seshat-content gridview child {{
  color: @seshat_text_primary;
  padding: {padding}px;
  border-radius: {radius}px;
}}

.seshat-content gridview child:hover {{
  background-color: @seshat_hover;
}}

.seshat-content gridview child:selected {{
  background-color: @seshat_selected;
}}

.seshat-search,
.seshat-search > revealer > box {{
  background-color: @seshat_surface;
  border-color: @seshat_border;
  box-shadow: none;
}}

.seshat-search > revealer > box {{
  border-bottom: 1px solid @seshat_border;
}}

.seshat-search entry image,
.seshat-pathbar entry image {{
  color: @seshat_text_muted;
}}

.seshat-search button {{
  color: @seshat_text_secondary;
  background-color: transparent;
}}

.seshat-search entry {{
  background-color: @seshat_app_background;
  color: @seshat_text_primary;
  border-radius: {radius}px;
}}

window.seshat popover > contents,
window.seshat popover > arrow {{
  background-color: @seshat_surface;
  color: @seshat_text_primary;
  border: 1px solid @seshat_border;
  border-radius: {radius}px;
}}

window.seshat popover modelbutton {{
  color: @seshat_text_primary;
  border-radius: {radius}px;
}}

window.seshat popover modelbutton:hover,
window.seshat popover modelbutton:selected {{
  background-color: @seshat_hover;
}}

window.seshat popover check:checked,
window.seshat popover radio:checked {{
  background-color: @seshat_accent;
  color: @seshat_surface;
  border-color: @seshat_accent;
}}

window.seshat popover separator {{
  background-color: @seshat_border;
}}

.seshat-content .dim-label,
.seshat-status {{
  color: @seshat_text_muted;
}}

.seshat-error {{
  color: @seshat_error;
}}

/* Spatial workspace. Every panel is main content, so every panel uses the
   surface token (the same one as the selected navigation item). The active
   panel is marked with an outline, never with a different fill. */
.seshat-workspace,
.seshat-strip {{
  background-color: @seshat_app_background;
}}

.seshat-strip {{
  padding: {spacing}px;
}}

.seshat-panel {{
  background-color: @seshat_surface;
  border-radius: {radius}px;
  box-shadow: inset 0 0 0 1px @seshat_border;
}}

.seshat-panel.seshat-panel-active {{
  box-shadow: inset 0 0 0 2px @seshat_accent;
}}

/* Every panel header has the same height, whatever it contains (branch
   badge, fork button, Focus Mode arrows): one row sized from the icon
   size, with compact buttons that cannot stretch it. */
.seshat-panel-header {{
  min-height: {header_row}px;
  padding: {spacing}px {padding}px;
  border-bottom: 1px solid @seshat_border;
}}

.seshat-panel-header button {{
  min-height: 0;
  min-width: 0;
  padding: 0 4px;
  margin: 0;
}}

.seshat-panel-header .seshat-branch-badge {{
  min-height: 0;
  padding: 0 6px;
}}

.seshat-panel-title {{
  color: @seshat_text_secondary;
}}

.seshat-panel-active .seshat-panel-title {{
  color: @seshat_text_primary;
  font-weight: bold;
}}

.seshat-branch-badge {{
  color: @seshat_accent;
  border: 1px solid @seshat_accent;
  border-radius: {radius}px;
  padding: 0 6px;
}}

/* Cards: panels of other branches, and far panels of this one. They are
   panels too, so they use the surface token like every panel. */
.seshat-card {{
  background-color: @seshat_surface;
  color: @seshat_text_secondary;
  border-radius: {radius}px;
  box-shadow: inset 0 0 0 1px @seshat_border;
  padding: {spacing}px {padding}px;
}}

.seshat-card:hover {{
  box-shadow: inset 0 0 0 1px @seshat_accent;
}}

.seshat-card.seshat-card-on-path .seshat-card-title {{
  color: @seshat_text_primary;
}}

/* Connectors draw in their CSS colour. */
.seshat-connector {{
  color: @seshat_border;
}}

.seshat-connector.seshat-connector-active {{
  color: @seshat_accent;
}}

.seshat-branch-stub {{
  color: @seshat_text_muted;
  min-height: 0;
  padding: 2px {padding}px;
}}

.seshat-content listview row.seshat-cursor,
.seshat-content listview row > .seshat-cursor,
.seshat-content gridview child > .seshat-cursor {{
  box-shadow: inset 2px 0 0 0 @seshat_focus;
}}

.seshat-branch-bar {{
  padding: 2px {padding}px;
}}

.seshat-branch {{
  border-radius: {radius}px;
  color: @seshat_text_secondary;
}}

.seshat-branch button {{
  color: inherit;
  min-height: 0;
  padding: 2px 6px;
}}

.seshat-branch.seshat-branch-active {{
  color: @seshat_text_primary;
  box-shadow: inset 0 -2px 0 0 @seshat_accent;
}}

.seshat-status {{
  padding: 2px {padding}px;
}}

.seshat-mode {{
  color: @seshat_accent;
  font-weight: bold;
}}

.seshat-mode.seshat-mode-visual {{
  color: @seshat_warning;
}}

.seshat-mode.seshat-mode-command {{
  color: @seshat_success;
}}

.seshat-focus-indicator {{
  color: @seshat_accent;
  border: 1px solid @seshat_accent;
  border-radius: {radius}px;
  padding: 0 6px;
}}

.seshat-command {{
  background-color: @seshat_surface;
  color: @seshat_text_primary;
  border-radius: {radius}px;
}}

.seshat-operations {{
  padding: {padding}px;
  background-color: @seshat_app_background;
  border-top: 1px solid @seshat_border;
}}

.seshat-operations > box {{
  padding: {spacing}px {padding}px;
  border-radius: {radius}px;
  background-color: @seshat_surface;
}}

.seshat-operations .seshat-error .dim-label {{
  color: @seshat_error;
}}

.seshat-operations progressbar > trough {{
  min-height: 6px;
  border-radius: 3px;
  background-color: @seshat_border;
}}

.seshat-operations progressbar > trough > progress {{
  min-height: 6px;
  border-radius: 3px;
  background-color: @seshat_accent;
}}

window.seshat-dialog {{
  background-color: @seshat_surface;
}}

window.seshat button.suggested-action {{
  background-color: @seshat_accent;
  color: @seshat_surface;
}}

window.seshat button.destructive-action {{
  background-color: @seshat_error;
  color: @seshat_surface;
}}

.seshat-status.seshat-notice {{
  color: @seshat_warning;
}}

.seshat-status {{
  padding: {spacing}px {padding}px;
  background-color: @seshat_app_background;
}}

*:focus-visible {{
  outline-color: @seshat_focus;
}}
"#,
        font_size = t.font_size,
        font_weight = t.font_weight,
        toolbar_height = g.toolbar_height,
        padding = g.padding,
        spacing = g.spacing,
        radius = g.border_radius,
        row_height = g.row_height,
        icon_size = g.icon_size,
        header_row = g.icon_size + 8,
    );
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{dark, light};

    /// Returns the declarations of the first rule whose selector list
    /// contains `selector`.
    fn rule_body<'a>(css: &'a str, selector: &str) -> &'a str {
        let mut rest = css;
        while let Some(open) = rest.find('{') {
            // Comments may precede a rule and contain commas: drop them.
            let selectors = rest[..open].rsplit("*/").next().unwrap_or_default();
            let close = open + rest[open..].find('}').expect("unterminated rule");
            let body = &rest[open + 1..close];
            if selectors
                .rsplit(';')
                .next()
                .unwrap_or_default()
                .split(',')
                .any(|s| s.trim() == selector)
            {
                return body;
            }
            rest = &rest[close + 1..];
        }
        panic!("no rule for {selector}");
    }

    fn background(body: &str) -> &str {
        body.lines()
            .find_map(|l| l.trim().strip_prefix("background-color:"))
            .expect("no background-color")
            .trim()
            .trim_end_matches(';')
    }

    #[test]
    fn selected_nav_and_content_share_the_surface_token() {
        for theme in [light(), dark()] {
            let css = render_css(&theme);
            let content = rule_body(&css, ".seshat-content");
            let selected = rule_body(&css, ".seshat-sidebar row:selected");
            assert_eq!(background(content), "@seshat_surface", "{}", theme.name);
            assert_eq!(background(selected), "@seshat_surface", "{}", theme.name);
        }
    }

    #[test]
    fn workspace_panels_use_the_shared_surface_token() {
        for theme in [light(), dark()] {
            let css = render_css(&theme);
            assert_eq!(
                background(rule_body(&css, ".seshat-panel")),
                "@seshat_surface"
            );
            // The active panel is an outline, never a different fill.
            let active = rule_body(&css, ".seshat-panel.seshat-panel-active");
            assert!(!active.contains("background"), "{active}");
            assert!(active.contains("@seshat_accent"), "{active}");
        }
    }

    #[test]
    fn cards_and_connectors_use_theme_tokens() {
        for theme in [light(), dark()] {
            let css = render_css(&theme);
            assert_eq!(
                background(rule_body(&css, ".seshat-card")),
                "@seshat_surface"
            );
            assert!(rule_body(&css, ".seshat-connector").contains("color: @seshat_border;"));
            assert!(
                rule_body(&css, ".seshat-connector.seshat-connector-active")
                    .contains("color: @seshat_accent;")
            );
        }
    }

    #[test]
    fn selected_nav_radius_is_exactly_12px() {
        for theme in [light(), dark()] {
            let css = render_css(&theme);
            let selected = rule_body(&css, ".seshat-sidebar row:selected");
            assert!(selected.contains("border-radius: 12px;"), "{selected}");
        }
    }

    #[test]
    fn custom_geometry_cannot_change_the_selected_nav_radius() {
        let mut theme = dark();
        theme.geometry.border_radius = 3;
        let css = render_css(&theme);
        let selected = rule_body(&css, ".seshat-sidebar row:selected");
        assert!(selected.contains("border-radius: 12px;"));
        assert!(rule_body(&css, ".seshat-sidebar row").contains("border-radius: 3px;"));
    }

    #[test]
    fn each_colour_is_defined_exactly_once() {
        let css = render_css(&light());
        let surface_value = light().colors.surface.to_css();
        assert_eq!(css.matches("@define-color seshat_surface ").count(), 1);
        // The raw value must not be duplicated into rules.
        assert_eq!(css.matches(surface_value.as_str()).count(), 1);
    }

    #[test]
    fn font_family_cannot_break_out_of_the_string() {
        let mut theme = light();
        theme.typography.font_family = "Evil\"; } * { color: red".to_owned();
        let css = render_css(&theme);
        assert!(css.contains("font-family: \"Evil; } * { color: red\";"));
    }
}
