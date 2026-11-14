# Writing themes

Seshat is fully themeable through small TOML files. Every colour, size and font in the UI comes from these tokens; components never hard-code styling.

## Where themes live

```
~/.config/seshat/themes/<id>.toml      ($XDG_CONFIG_HOME/seshat/themes)
```

The file stem is the theme's id. Seshat watches this folder, so saving a file reloads it immediately. The theme menu (palette icon) lists custom themes and has **Open Themes Folder** and **Reload Themes**. Your choice is saved in `~/.config/seshat/settings.toml`.

## Format

Start from [`docs/themes/nord.toml`](../themes/nord.toml) or [`docs/themes/catppuccin-mocha.toml`](../themes/catppuccin-mocha.toml) (installed with the package as `share/doc/seshat/examples/themes/catppuccin-mocha.toml`). Every key is optional: anything you leave out comes from the base theme.

| Key | Values |
|---|---|
| `name` | Display name (defaults to the file name) |
| `extends` | `"light"` (default) or `"dark"`: the base theme, which also tells Seshat whether the theme is dark |

### `[colors]`

`"#rrggbb"` or `"#rrggbbaa"`.

| Token | Used for |
|---|---|
| `app_background` | Window, toolbar, status bar |
| `surface` | **Main content pane and the selected navigation item** (one token by design) |
| `navigation_surface` | Navigation pane |
| `text_primary`, `text_secondary`, `text_muted` | Text hierarchy |
| `accent` | Selected-item icon, checked menu items |
| `border` | Outlines, separators |
| `hover` | Hovered rows and buttons |
| `selected` | Selected files |
| `focus` | Keyboard focus outline |
| `error`, `warning`, `success` | Messages |

### `[geometry]`

Whole numbers in pixels: `border_radius` (0–24), `padding` (0–32), `spacing` (0–32), `row_height` (20–96), `toolbar_height` (32–96), `icon_size` (12–64).

### `[typography]`

`font_family` (a font name), `font_size` (6–36 pt), `font_weight` (100–900 in steps of 100), `line_height` (0.8–3.0).

## Fixed rules

Some parts of the design are deliberately not configurable:

- The selected navigation item always has a **12px** radius on all corners.
- The selected navigation item's background is always the `surface` token, the same as the content pane, so they cannot drift apart.

A theme that tries to set these gets an "unknown setting" warning.

## Validation

Themes are validated when they load:

- **Invalid TOML** (for example an unquoted string): the theme is not loaded, and the status bar names the file and the error. Other themes are unaffected.
- **An invalid value** (bad colour, out-of-range number, unknown key): that value falls back to the base theme, and the rest of the theme still applies. The theme is marked ⚠ in the menu, the status bar reports the number of issues, and each issue is logged:

  ```
  WARN theme=nord path=…/nord.toml geometry.border_radius: expected a whole number from 0 to 24, found 100
  ```
- **Low contrast:** if `text_primary` reaches less than 4.5:1 (WCAG AA) against `surface`, `navigation_surface` or `app_background`, you get a warning. The theme is still applied.

Run Seshat with `RUST_LOG=warn` (the default is `info`) to see the warnings in the terminal.

To check a theme without starting Seshat, run `seshat --check-theme path/to/theme.toml`. It loads the file exactly as Seshat would, prints every problem, and exits with status 0 only if there are none.

## A theme given at launch

`seshat --theme-file path/to/theme.toml` starts with the theme in that file. The file uses the same format and validation as any other theme, and it doesn't need to be in the themes folder. The Home Manager module uses it to pass on a theme generated from Stylix.

- It is listed first in the theme menu, under **Launch Theme**, and it wins over your saved theme every time Seshat starts with the option.
- You can pick another theme from the menu. That choice is saved as usual, but the launch theme itself is never saved, so starting without the option uses your saved theme.
- If the file can't be read or parsed, the status bar says so and the saved theme is used. **Reload Themes** reads the file again.

## How it works

`ThemeRegistry` (presentation) parses the files that `FileThemeRepository` (infrastructure) reads, including the launch theme, which is kept apart from the custom themes as `ThemeChoice::Provided` and never saved. `render_css` turns the resolved `Theme` into a single stylesheet in which each colour is defined once with `@define-color` and only referenced by rules. Unit tests in `theme/css.rs` and `theme/custom.rs` enforce the fixed rules for every built-in theme and for overrides.
