use super::{Color, ColorTokens, Geometry, Theme, Typography};

pub fn light() -> Theme {
    Theme {
        name: "Light".to_owned(),
        dark: false,
        colors: ColorTokens {
            app_background: Color::rgb(0xeb, 0xeb, 0xee),
            surface: Color::rgb(0xff, 0xff, 0xff),
            navigation_surface: Color::rgb(0xeb, 0xeb, 0xee),
            text_primary: Color::rgb(0x1f, 0x1f, 0x23),
            text_secondary: Color::rgb(0x4a, 0x4a, 0x52),
            text_muted: Color::rgb(0x7a, 0x7a, 0x84),
            accent: Color::rgb(0x35, 0x84, 0xe4),
            border: Color::rgba(0x00, 0x00, 0x00, 0x1f),
            hover: Color::rgba(0x00, 0x00, 0x00, 0x0f),
            selected: Color::rgba(0x35, 0x84, 0xe4, 0x2e),
            focus: Color::rgb(0x35, 0x84, 0xe4),
            error: Color::rgb(0xc0, 0x1c, 0x28),
            warning: Color::rgb(0xb5, 0x83, 0x0f),
            success: Color::rgb(0x26, 0xa2, 0x69),
        },
        geometry: Geometry::default(),
        typography: Typography::default(),
    }
}

pub fn dark() -> Theme {
    Theme {
        name: "Dark".to_owned(),
        dark: true,
        colors: ColorTokens {
            app_background: Color::rgb(0x1a, 0x1a, 0x1d),
            surface: Color::rgb(0x26, 0x26, 0x2b),
            navigation_surface: Color::rgb(0x1a, 0x1a, 0x1d),
            text_primary: Color::rgb(0xf2, 0xf2, 0xf5),
            text_secondary: Color::rgb(0xc4, 0xc4, 0xcc),
            text_muted: Color::rgb(0x8c, 0x8c, 0x96),
            accent: Color::rgb(0x78, 0xae, 0xed),
            border: Color::rgba(0xff, 0xff, 0xff, 0x1f),
            hover: Color::rgba(0xff, 0xff, 0xff, 0x0f),
            selected: Color::rgba(0x78, 0xae, 0xed, 0x33),
            focus: Color::rgb(0x78, 0xae, 0xed),
            error: Color::rgb(0xff, 0x7b, 0x63),
            warning: Color::rgb(0xf8, 0xe4, 0x5c),
            success: Color::rgb(0x8f, 0xf0, 0xa4),
        },
        geometry: Geometry::default(),
        typography: Typography::default(),
    }
}
