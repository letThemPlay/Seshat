use super::Color;

/// A complete set of design tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    pub colors: ColorTokens,
    pub geometry: Geometry,
    pub typography: Typography,
}

/// Colour tokens. `surface` is the single source for both the main content
/// background and the selected navigation item background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTokens {
    pub app_background: Color,
    pub surface: Color,
    pub navigation_surface: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub border: Color,
    pub hover: Color,
    pub selected: Color,
    pub focus: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// Radius for generic controls and rows (not the selected nav item).
    pub border_radius: u32,
    pub padding: u32,
    pub spacing: u32,
    pub row_height: u32,
    pub toolbar_height: u32,
    pub icon_size: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    pub font_family: String,
    /// Points.
    pub font_size: u32,
    pub font_weight: u32,
    pub line_height: Option<f32>,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            border_radius: 8,
            padding: 8,
            spacing: 6,
            row_height: 34,
            toolbar_height: 46,
            icon_size: 16,
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: "sans-serif".to_owned(),
            font_size: 11,
            font_weight: 400,
            line_height: None,
        }
    }
}
