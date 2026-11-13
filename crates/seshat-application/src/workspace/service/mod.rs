//! The workspace service: one facade over navigation, panel listings,
//! cursors and selection, and panel settings. Each concern is a child
//! module adding methods to [`WorkspaceService`]; this module holds the
//! state and the steps they share (loading, evicting, forgetting panels).

mod found;
mod loading;
mod navigation;
mod selection;
mod settings;

use seshat_domain::entities::Location;
use seshat_domain::workspace::{PanelId, PanelState, Workspace};

use super::cache::ListingCache;
use super::listing::PanelListing;
use super::undo::WorkspaceUndo;

/// Listings kept in memory (and watched) at most, by default. Panels beyond
/// this keep their logical state and are re-read when shown again.
pub const DEFAULT_LISTING_CAPACITY: usize = 16;

/// Work the presentation shell must carry out for the service. The service
/// itself does no I/O, which keeps it deterministic and testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// List `location` and report back with [`WorkspaceService::listing_loaded`].
    Load {
        panel: PanelId,
        location: Location,
        generation: u64,
    },
    /// Watch `location`; report changes with
    /// [`WorkspaceService::directory_changed`] and this generation.
    Watch {
        panel: PanelId,
        location: Location,
        generation: u64,
    },
    /// Stop watching (drop the watch guard).
    Unwatch { panel: PanelId },
    /// Open files with their default applications.
    Launch(Vec<Location>),
    /// The workspace changed in a way worth saving.
    Persist,
}

/// One panel's share of the workspace-wide selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionGroup {
    pub panel: PanelId,
    pub folder: Location,
    pub items: Vec<Location>,
}

/// Workspace navigation as an application service: every keyboard, mouse
/// and command-line action ends up here, so they all share one behaviour.
///
/// It owns the [`Workspace`] and the panels' listings, and returns the I/O
/// it needs as [`Effect`]s.
#[derive(Debug)]
pub struct WorkspaceService {
    workspace: Workspace,
    /// The panels' listings in memory, least recently used evicted first.
    listings: ListingCache,
    /// Settings a new root panel starts with (global preferences).
    defaults: PanelState,
    effects: Vec<Effect>,
    /// Whether the cursor wraps around at the ends of a list.
    wrap_cursor: bool,
    /// Reversible workspace changes of this session (decision B5).
    undo: WorkspaceUndo,
}

impl WorkspaceService {
    pub fn new(workspace: Workspace, defaults: PanelState) -> Self {
        let mut service = Self {
            workspace,
            listings: ListingCache::new(DEFAULT_LISTING_CAPACITY),
            defaults: defaults.inherit(),
            effects: Vec::new(),
            wrap_cursor: false,
            undo: WorkspaceUndo::default(),
        };
        service.after_navigation();
        service
    }

    /// A workspace with a single panel at `home`.
    pub fn starting_at(home: Location, defaults: PanelState) -> Self {
        let state = defaults.inherit();
        Self::new(Workspace::new(home, state), defaults)
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.listings.set_capacity(capacity);
        self.evict();
    }

    pub fn set_wrap_cursor(&mut self, wrap: bool) {
        self.wrap_cursor = wrap;
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn defaults(&self) -> &PanelState {
        &self.defaults
    }

    pub fn listing(&self, panel: PanelId) -> Option<&PanelListing> {
        self.listings.get(&panel)
    }

    /// A loaded listing of `location` from any open panel, so completion can
    /// use it instead of reading the folder again.
    pub fn listing_for(&self, location: &Location) -> Option<&PanelListing> {
        self.workspace
            .panels()
            .filter(|p| p.location() == location)
            .filter_map(|p| self.listings.get(&p.id()))
            .find(|l| l.loaded)
    }

    pub fn active_panel(&self) -> PanelId {
        self.workspace.active_panel()
    }

    pub fn active_location(&self) -> &Location {
        self.location(self.active_panel())
            .expect("the active panel exists")
    }

    pub fn location(&self, panel: PanelId) -> Option<&Location> {
        self.workspace.panel(panel).map(|p| p.location())
    }

    pub fn state(&self, panel: PanelId) -> Option<&PanelState> {
        self.workspace.panel(panel).map(|p| &p.state)
    }

    /// Takes the effects produced since the last call.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    fn next_generation(&mut self) -> u64 {
        self.listings.next_generation()
    }

    /// Drops the listings and watches of panels removed from the workspace,
    /// or whose folder was renamed or replaced (their listing is stale).
    fn forget_panels(&mut self, removed: &[PanelId]) {
        for &panel in removed {
            if self.listings.remove(panel) {
                self.effects.push(Effect::Unwatch { panel });
            }
        }
    }

    /// Runs a workspace navigation step, then loads the active panel,
    /// evicts listings beyond the cache and saves.
    fn navigate<T>(&mut self, step: impl FnOnce(&mut Workspace) -> T) -> T {
        let result = step(&mut self.workspace);
        self.after_navigation();
        result
    }

    /// [`Self::navigate`] for a step that can fail: on failure nothing
    /// else happens.
    fn try_navigate<T, E>(
        &mut self,
        step: impl FnOnce(&mut Workspace) -> Result<T, E>,
    ) -> Result<T, E> {
        let result = step(&mut self.workspace)?;
        self.after_navigation();
        Ok(result)
    }

    fn after_navigation(&mut self) {
        let active = self.active_panel();
        self.ensure_listing(active);
        self.evict();
        self.effects.push(Effect::Persist);
    }

    fn ensure_listing(&mut self, panel: PanelId) {
        if self.listings.touch(panel) {
            return;
        }
        let Some(p) = self.workspace.panel(panel) else {
            return;
        };
        if let Some(reason) = p.unavailable() {
            // Nothing to read until the user retries (Refresh).
            self.listings.insert_failed(panel, reason.into());
            return;
        }
        let location = p.location().clone();
        let generation = self.listings.start_loading(panel);
        // Watch before listing so changes in between are not missed.
        self.effects.push(Effect::Watch {
            panel,
            location: location.clone(),
            generation,
        });
        self.effects.push(Effect::Load {
            panel,
            location,
            generation,
        });
    }

    /// Drops the least recently used listings beyond capacity, except for
    /// shown panels and the active one. Their logical state is kept.
    fn evict(&mut self) {
        let active = self.active_panel();
        for panel in self.listings.evict(active) {
            self.effects.push(Effect::Unwatch { panel });
        }
    }
}
