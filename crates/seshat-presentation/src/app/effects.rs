//! Carries out the workspace service's I/O effects: listing and watching
//! folders, opening files, and saving the workspace, with live reloads
//! debounced and saves batched. The service only says what to do
//! ([`Effect`]); this owns the ports that do it and the state of what is
//! in flight (watch guards, pending reloads, a pending save).
//!
//! Results come back to the window as messages, so the service is only
//! ever changed by `App::update`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use relm4::ComponentSender;
use seshat_application::ports::{DirectoryWatcher, Launcher, WatchGuard};
use seshat_application::use_cases::ListDirectory;
use seshat_application::workspace::{Effect, WorkspaceStore};
use seshat_domain::entities::Location;
use seshat_domain::workspace::{PanelId, Workspace};

use super::{App, ListingResult, Msg, RELOAD_DEBOUNCE};

/// The longest a live reload waits, however large the folder.
const MAX_RELOAD_DEBOUNCE: Duration = Duration::from_secs(3);

/// Workspace changes are saved this long after the last one.
const SAVE_DELAY: Duration = Duration::from_secs(1);

/// Live-reload debounce for a folder of `entries` entries: at least
/// [`RELOAD_DEBOUNCE`], longer for very large folders so a constantly
/// changing one cannot keep the UI busy.
pub(super) fn reload_delay(entries: usize) -> Duration {
    // About 3 ms per thousand entries rebuilt, times three.
    let estimate = Duration::from_micros(entries as u64 * 9);
    estimate.clamp(RELOAD_DEBOUNCE, MAX_RELOAD_DEBOUNCE)
}

pub(super) struct EffectRunner {
    list_directory: ListDirectory,
    directory_watcher: Rc<dyn DirectoryWatcher>,
    launcher: Rc<dyn Launcher>,
    workspace_store: Rc<dyn WorkspaceStore>,
    /// Watched panels; dropping a guard stops its watch.
    watches: HashMap<PanelId, WatchGuard>,
    /// Panels with a debounced reload scheduled.
    reload_pending: HashSet<PanelId>,
    /// A save is scheduled.
    save_pending: bool,
}

impl EffectRunner {
    pub(super) fn new(
        list_directory: ListDirectory,
        directory_watcher: Rc<dyn DirectoryWatcher>,
        launcher: Rc<dyn Launcher>,
        workspace_store: Rc<dyn WorkspaceStore>,
    ) -> Self {
        Self {
            list_directory,
            directory_watcher,
            launcher,
            workspace_store,
            watches: HashMap::new(),
            reload_pending: HashSet::new(),
            save_pending: false,
        }
    }

    /// The watcher, for the theme controller's themes-folder watch.
    pub(super) fn watcher(&self) -> Rc<dyn DirectoryWatcher> {
        self.directory_watcher.clone()
    }

    pub(super) fn run(&mut self, effects: Vec<Effect>, sender: &ComponentSender<App>) {
        for effect in effects {
            match effect {
                Effect::Load {
                    panel,
                    location,
                    generation,
                } => {
                    tracing::debug!(%panel, "listing requested");
                    let use_case = self.list_directory.clone();
                    let sender = sender.clone();
                    relm4::spawn_local(async move {
                        let result = use_case.execute(&location).await;
                        sender.input_sender().emit(Msg::PanelLoaded {
                            panel,
                            generation,
                            result: ListingResult(result),
                        });
                    });
                }
                Effect::Watch {
                    panel,
                    location,
                    generation,
                } => {
                    let watch_sender = sender.clone();
                    match self.directory_watcher.watch(
                        &location,
                        Box::new(move |change| {
                            watch_sender.input_sender().emit(Msg::PanelChanged {
                                panel,
                                generation,
                                change,
                            })
                        }),
                    ) {
                        Ok(guard) => {
                            self.watches.insert(panel, guard);
                        }
                        Err(err) => {
                            self.watches.remove(&panel);
                            tracing::debug!(%panel, error = %err, "cannot watch; no live updates");
                        }
                    }
                }
                Effect::Unwatch { panel } => {
                    self.watches.remove(&panel);
                }
                Effect::Launch(files) => self.launch(files, sender.input_sender()),
                Effect::Persist => self.schedule_save(sender),
            }
        }
    }

    /// Opens files in their default applications; failures come back as
    /// [`Msg::OpenFailed`].
    pub(super) fn launch(&self, files: Vec<Location>, input: &relm4::Sender<Msg>) {
        for location in files {
            let launcher = self.launcher.clone();
            let sender = input.clone();
            relm4::spawn_local(async move {
                if let Err(err) = launcher.open(&location).await {
                    sender.emit(Msg::OpenFailed(err));
                }
            });
        }
    }

    /// Schedules a reload of `panel` after `delay`, unless one is already
    /// pending; it arrives as [`Msg::ReloadPanel`].
    pub(super) fn schedule_reload(
        &mut self,
        panel: PanelId,
        generation: u64,
        delay: Duration,
        sender: &ComponentSender<App>,
    ) {
        if !self.reload_pending.insert(panel) {
            return;
        }
        let sender = sender.clone();
        gtk::glib::timeout_add_local_once(delay, move || {
            sender
                .input_sender()
                .emit(Msg::ReloadPanel { panel, generation });
        });
    }

    /// A scheduled reload of `panel` is due, or the panel is gone.
    pub(super) fn reload_done(&mut self, panel: PanelId) {
        self.reload_pending.remove(&panel);
    }

    /// Batches saves: one [`Msg::SaveWorkspace`] after [`SAVE_DELAY`].
    fn schedule_save(&mut self, sender: &ComponentSender<App>) {
        if self.save_pending {
            return;
        }
        self.save_pending = true;
        let sender = sender.clone();
        gtk::glib::timeout_add_local_once(SAVE_DELAY, move || {
            sender.input_sender().emit(Msg::SaveWorkspace);
        });
    }

    /// The scheduled save is due ([`Msg::SaveWorkspace`]).
    pub(super) fn save_due(&mut self) {
        self.save_pending = false;
    }

    /// Saves `workspace` now (in the background; see [`Self::flush`]).
    pub(super) fn save(&self, workspace: &Workspace) {
        let started = Instant::now();
        match self.workspace_store.save(workspace) {
            Ok(()) => tracing::debug!(
                elapsed_us = started.elapsed().as_micros() as u64,
                panels = workspace.panel_count(),
                "workspace saved"
            ),
            Err(err) => tracing::warn!(error = %err, "workspace not saved"),
        }
    }

    /// Waits for the last save to be written.
    pub(super) fn flush(&self) {
        self.workspace_store.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_delay_grows_with_the_folder_within_bounds() {
        assert_eq!(reload_delay(0), RELOAD_DEBOUNCE);
        assert_eq!(reload_delay(1_000), RELOAD_DEBOUNCE, "small folders");
        assert_eq!(reload_delay(100_000), Duration::from_millis(900));
        assert_eq!(reload_delay(10_000_000), MAX_RELOAD_DEBOUNCE);
    }
}
