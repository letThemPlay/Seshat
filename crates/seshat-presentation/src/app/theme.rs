//! Themes: the chosen theme, the custom themes found in the themes folder,
//! reloading them when the folder changes, and applying the stylesheet.

use std::rc::Rc;

use relm4::ComponentSender;
use seshat_application::ports::{DirectoryWatcher, ThemeRepository, WatchGuard};
use seshat_application::preferences::ThemeChoice;
use seshat_domain::entities::Location;

use super::{App, Msg, NoticeCx, RELOAD_DEBOUNCE};
use crate::actions;
use crate::notice::{self, Notice};
use crate::theme::{Theme, ThemeRegistry, render_css};

/// What the window must do after a theme message.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ThemeOutcome {
    Stay,
    /// The chosen theme changed: save the preferences.
    SavePreferences,
    /// Open the themes folder (decision D3).
    Open(Location),
}

#[derive(Debug)]
pub enum ThemeMsg {
    /// A theme was picked from the theme menu.
    Set(ThemeChoice),
    /// Read the themes folder again.
    Reload,
    /// Files in the themes folder changed; reloads after a short delay.
    FilesChanged,
    OpenFolder,
    SystemColorSchemeChanged,
}

pub(super) struct ThemeController {
    repository: Rc<dyn ThemeRepository>,
    /// The theme shown now.
    choice: ThemeChoice,
    /// The theme to save: the last saved choice picked, never
    /// [`ThemeChoice::Provided`]. Keeps the settings independent of whether
    /// Seshat was started with `--theme-file`.
    saved: ThemeChoice,
    registry: ThemeRegistry,
    watch: Option<WatchGuard>,
    reload_pending: bool,
    css_provider: gtk::CssProvider,
    menu_button: gtk::MenuButton,
}

impl ThemeController {
    pub(super) fn new(
        repository: Rc<dyn ThemeRepository>,
        choice: ThemeChoice,
        css_provider: gtk::CssProvider,
        menu_button: gtk::MenuButton,
    ) -> Self {
        Self {
            repository,
            saved: choice.clone(),
            choice,
            registry: ThemeRegistry::default(),
            watch: None,
            reload_pending: false,
            css_provider,
            menu_button,
        }
    }

    pub(super) fn choice(&self) -> &ThemeChoice {
        &self.choice
    }

    /// The choice to save in the settings.
    pub(super) fn saved_choice(&self) -> &ThemeChoice {
        &self.saved
    }

    /// Loads the custom themes, starts watching their folder and applies
    /// the chosen theme. A theme given with `--theme-file` wins over the
    /// saved choice at launch.
    pub(super) fn start(&mut self, watcher: &dyn DirectoryWatcher, cx: &mut NoticeCx<'_>) {
        self.load(cx);
        self.choice = launch_choice(&self.registry, &self.saved);
        self.watch_folder(watcher, cx.sender);
        self.apply(cx);
    }

    /// Handles `msg`, and says what the window must do next.
    pub(super) fn update(&mut self, msg: ThemeMsg, cx: &mut NoticeCx<'_>) -> ThemeOutcome {
        match msg {
            ThemeMsg::Set(choice) => {
                if choice.is_saved() {
                    self.saved = choice.clone();
                }
                self.choice = choice;
                self.apply(cx);
                return ThemeOutcome::SavePreferences;
            }
            ThemeMsg::Reload => {
                self.reload_pending = false;
                self.load(cx);
                self.apply(cx);
            }
            ThemeMsg::FilesChanged if !self.reload_pending => {
                self.reload_pending = true;
                let sender = cx.sender.clone();
                gtk::glib::timeout_add_local_once(RELOAD_DEBOUNCE, move || {
                    sender.input_sender().emit(Msg::Theme(ThemeMsg::Reload));
                });
            }
            ThemeMsg::FilesChanged => {}
            ThemeMsg::OpenFolder => match self.repository.ensure_directory() {
                Ok(directory) => return ThemeOutcome::Open(Location::Path(directory)),
                Err(err) => {
                    *cx.notice = Some(notice::themes_folder_failed(&err));
                }
            },
            ThemeMsg::SystemColorSchemeChanged => {
                if self.choice == ThemeChoice::System {
                    self.apply(cx);
                }
            }
        }
        ThemeOutcome::Stay
    }

    fn load(&mut self, cx: &mut NoticeCx<'_>) {
        self.registry = ThemeRegistry::from_sources(self.repository.load_all())
            .with_provided(self.repository.load_provided());
        self.menu_button
            .set_menu_model(Some(&actions::theme_menu(&self.registry)));
        if let Some(error) = self.registry.errors().first() {
            *cx.notice = Some(notice::theme_not_loaded(&error.id, &error.message));
        }
        self.choice = reloaded_choice(&self.registry, &self.choice, &self.saved);
    }

    /// Reloads themes when files in the themes folder change.
    fn watch_folder(&mut self, watcher: &dyn DirectoryWatcher, sender: &ComponentSender<App>) {
        let directory = match self.repository.ensure_directory() {
            Ok(directory) => directory,
            Err(err) => {
                tracing::debug!(error = %err, "cannot create themes folder; not watching");
                return;
            }
        };
        let sender = sender.clone();
        self.watch = watcher
            .watch(
                &Location::Path(directory),
                Box::new(move |_| {
                    sender
                        .input_sender()
                        .emit(Msg::Theme(ThemeMsg::FilesChanged))
                }),
            )
            .ok();
    }

    fn apply(&mut self, cx: &mut NoticeCx<'_>) {
        let (theme, notice) = pick(&self.registry, &self.choice, system_prefers_dark());
        if notice.is_some() {
            *cx.notice = notice;
        }
        tracing::debug!(theme = %theme.name, choice = ?self.choice, "applying theme");
        self.css_provider.load_from_string(&render_css(&theme));
    }
}

/// The theme to show at launch: the one given with `--theme-file` if it
/// loaded, otherwise the saved choice.
fn launch_choice(registry: &ThemeRegistry, saved: &ThemeChoice) -> ThemeChoice {
    if registry.provided().is_some() {
        ThemeChoice::Provided
    } else {
        saved.clone()
    }
}

/// The theme to show after reloading: a provided theme that no longer
/// loads gives way to the saved choice (the load error says why).
fn reloaded_choice(
    registry: &ThemeRegistry,
    choice: &ThemeChoice,
    saved: &ThemeChoice,
) -> ThemeChoice {
    if *choice == ThemeChoice::Provided && registry.provided().is_none() {
        saved.clone()
    } else {
        choice.clone()
    }
}

/// The theme to show for `choice`, and what to tell the user about it: a
/// custom theme that is gone falls back to the system theme, and a custom
/// theme with problems says so.
fn pick(
    registry: &ThemeRegistry,
    choice: &ThemeChoice,
    system_dark: bool,
) -> (Theme, Option<Notice>) {
    let Some(theme) = registry.resolve(choice, system_dark) else {
        let notice = notice::theme_unavailable(choice.id().trim_start_matches("custom:"));
        let fallback = registry
            .resolve(&ThemeChoice::System, system_dark)
            .expect("built-in themes always resolve");
        return (fallback, Some(notice));
    };
    let loaded = match choice {
        ThemeChoice::Custom(id) => registry.custom(id),
        ThemeChoice::Provided => registry.provided(),
        _ => None,
    };
    let notice = loaded
        .filter(|custom| !custom.warnings.is_empty())
        .map(|custom| notice::theme_has_issues(&custom.theme.name, custom.warnings.len()));
    (theme, notice)
}

fn system_prefers_dark() -> bool {
    gtk::Settings::default()
        .is_some_and(|s| s.gtk_interface_color_scheme() == gtk::InterfaceColorScheme::Dark)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use seshat_application::ports::ThemeSource;

    use super::*;

    fn registry() -> ThemeRegistry {
        ThemeRegistry::from_sources(vec![
            ThemeSource {
                id: "good".into(),
                path: PathBuf::from("/themes/good.toml"),
                contents: Ok("name = \"Good\"\nextends = \"dark\"".into()),
            },
            ThemeSource {
                id: "odd".into(),
                path: PathBuf::from("/themes/odd.toml"),
                contents: Ok("name = \"Odd\"\n[colors]\naccent = \"not a colour\"".into()),
            },
        ])
    }

    #[test]
    fn built_in_and_valid_custom_themes_apply_quietly() {
        let registry = registry();
        let (theme, notice) = pick(&registry, &ThemeChoice::Dark, false);
        assert!(theme.dark);
        assert_eq!(notice, None);
        let (theme, notice) = pick(&registry, &ThemeChoice::System, true);
        assert!(theme.dark, "System follows the system preference");
        assert_eq!(notice, None);
        let (theme, notice) = pick(&registry, &ThemeChoice::Custom("good".into()), false);
        assert_eq!(theme.name, "Good");
        assert_eq!(notice, None);
    }

    #[test]
    fn a_missing_custom_theme_falls_back_to_the_system_theme() {
        let (theme, notice) = pick(&registry(), &ThemeChoice::Custom("gone".into()), true);
        assert!(theme.dark, "the system prefers dark");
        assert_eq!(
            notice.as_ref().map(Notice::text),
            Some("Theme “gone” is not available; following the system theme.")
        );
    }

    fn provided(contents: Result<&str, &str>) -> Option<ThemeSource> {
        Some(ThemeSource {
            id: "theme".into(),
            path: PathBuf::from("/nix/store/abc-theme.toml"),
            contents: contents.map(str::to_owned).map_err(str::to_owned),
        })
    }

    #[test]
    fn a_provided_theme_wins_at_launch_over_the_saved_choice() {
        let saved = ThemeChoice::Custom("good".into());
        let with = registry().with_provided(provided(Ok("name = \"Stylix\"")));
        assert_eq!(launch_choice(&with, &saved), ThemeChoice::Provided);
        assert_eq!(launch_choice(&registry(), &saved), saved, "no --theme-file");
        let broken = registry().with_provided(provided(Err("not found")));
        assert_eq!(launch_choice(&broken, &saved), saved, "unreadable file");
    }

    #[test]
    fn a_provided_theme_that_stops_loading_gives_way_to_the_saved_choice() {
        let saved = ThemeChoice::Dark;
        let gone = registry().with_provided(provided(Err("not found")));
        assert_eq!(
            reloaded_choice(&gone, &ThemeChoice::Provided, &saved),
            saved
        );
        let fine = registry().with_provided(provided(Ok("")));
        assert_eq!(
            reloaded_choice(&fine, &ThemeChoice::Provided, &saved),
            ThemeChoice::Provided
        );
        assert_eq!(
            reloaded_choice(&gone, &ThemeChoice::Light, &saved),
            ThemeChoice::Light,
            "other choices are left alone"
        );
    }

    #[test]
    fn the_provided_theme_applies_and_reports_its_problems() {
        let registry = registry().with_provided(Some(ThemeSource {
            id: "abc-seshat-theme".into(),
            path: PathBuf::from("/nix/store/abc-seshat-theme.toml"),
            contents: Ok("name = \"Stylix\"\n[colors]\nsurface = \"nope\"".into()),
        }));
        let (theme, notice) = pick(&registry, &ThemeChoice::Provided, false);
        assert_eq!(theme.name, "Stylix");
        assert_eq!(
            notice.as_ref().map(Notice::text),
            Some("Theme “Stylix” has 1 issue; see the log for details.")
        );
    }

    #[test]
    fn a_custom_theme_with_problems_says_so() {
        let (theme, notice) = pick(&registry(), &ThemeChoice::Custom("odd".into()), false);
        assert_eq!(theme.name, "Odd");
        assert_eq!(
            notice.as_ref().map(Notice::text),
            Some("Theme “Odd” has 1 issue; see the log for details.")
        );
    }
}
