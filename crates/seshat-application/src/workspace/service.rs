use std::collections::{HashMap, HashSet};
use std::path::Path;

use seshat_domain::entities::{FileEntry, Location};
use seshat_domain::view::{SortKey, ViewMode};
use seshat_domain::workspace::{
    BranchId, Opened, PanelId, PanelState, ScrollAnchor, Unavailability, Workspace, WorkspaceError,
};

use super::listing::{Content, PanelListing};
use crate::ports::{DirectoryChange, ListingError};
use crate::view::arrange;

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
    listings: HashMap<PanelId, PanelListing>,
    /// Settings a new root panel starts with (global preferences).
    defaults: PanelState,
    capacity: usize,
    /// Panels the presentation is currently rendering; never evicted.
    visible: HashSet<PanelId>,
    effects: Vec<Effect>,
    generation: u64,
    clock: u64,
    /// Whether the cursor wraps around at the ends of a list.
    wrap_cursor: bool,
}

impl WorkspaceService {
    pub fn new(workspace: Workspace, defaults: PanelState) -> Self {
        let mut service = Self {
            workspace,
            listings: HashMap::new(),
            defaults: defaults.inherit(),
            capacity: DEFAULT_LISTING_CAPACITY,
            visible: HashSet::new(),
            effects: Vec::new(),
            generation: 0,
            clock: 0,
            wrap_cursor: false,
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
        self.capacity = capacity.max(1);
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

    // ── Navigation ──────────────────────────────────────────────────────

    pub fn activate_panel(&mut self, panel: PanelId) -> Result<bool, WorkspaceError> {
        let changed = self.workspace.activate_panel(panel)?;
        self.after_navigation();
        Ok(changed)
    }

    pub fn move_left(&mut self) -> bool {
        let changed = self.workspace.move_left();
        self.after_navigation();
        changed
    }

    pub fn move_right(&mut self) -> bool {
        let changed = self.workspace.move_right();
        self.after_navigation();
        changed
    }

    /// Backspace / "parent folder": the parent panel, or for a root panel
    /// a new parent panel above it.
    pub fn go_parent(&mut self) -> bool {
        if self.workspace.move_left() {
            self.after_navigation();
            return true;
        }
        let root = self.active_panel();
        let changed = self.workspace.insert_parent(root).is_ok();
        self.after_navigation();
        changed
    }

    pub fn go_back(&mut self) -> bool {
        let changed = self.workspace.go_back();
        self.after_navigation();
        changed
    }

    pub fn go_forward(&mut self) -> bool {
        let changed = self.workspace.go_forward();
        self.after_navigation();
        changed
    }

    pub fn cycle_branch(&mut self, forward: bool) -> bool {
        let changed = self.workspace.cycle_branch(forward);
        self.after_navigation();
        changed
    }

    pub fn activate_branch(&mut self, branch: BranchId) -> Result<bool, WorkspaceError> {
        let changed = self.workspace.activate_branch(branch)?;
        self.after_navigation();
        Ok(changed)
    }

    /// Closes a branch; listings of removed panels are dropped. Never
    /// touches files, and running operations are unaffected (they hold
    /// their own paths).
    pub fn close_branch(&mut self, branch: BranchId) -> Result<Vec<PanelId>, WorkspaceError> {
        let removed = self.workspace.close_branch(branch)?;
        for panel in &removed {
            self.visible.remove(panel);
            if self.listings.remove(panel).is_some() {
                self.effects.push(Effect::Unwatch { panel: *panel });
            }
        }
        self.after_navigation();
        Ok(removed)
    }

    /// Opens `entry` from `panel`. This is the single entry point for
    /// opening: `l`, `Enter`, double-click and the command line all come
    /// here, so they resolve navigation identically.
    ///
    /// Folders are resolved by the workspace (context-aware navigation):
    /// continue the active branch, reactivate an existing branch, or create
    /// a new one; see [`Workspace::open_directory`]. Files are launched.
    /// Returns how a folder was resolved, or `None` for a file.
    ///
    /// [`Workspace::open_directory`]: seshat_domain::workspace::Workspace::open_directory
    pub fn open_entry(
        &mut self,
        panel: PanelId,
        entry: &FileEntry,
    ) -> Result<Option<Opened>, WorkspaceError> {
        if !entry.is_browsable() {
            self.effects
                .push(Effect::Launch(vec![entry.location.clone()]));
            return Ok(None);
        }
        self.workspace.activate_panel(panel)?;
        let opened =
            self.workspace
                .open_directory(panel, entry.location.clone(), entry.identity, None)?;
        if let Some(state) = self.workspace.state_mut(panel) {
            state.cursor = Some(entry.location.clone());
        }
        // Renamed or replaced folders: their listings and watches belong to
        // the old folder and are read again.
        for &refreshed in &opened.refreshed {
            self.visible.remove(&refreshed);
            if self.listings.remove(&refreshed).is_some() {
                self.effects.push(Effect::Unwatch { panel: refreshed });
            }
        }
        tracing::debug!(
            resolution = ?opened.resolution,
            branch = %opened.branch,
            panel = %opened.active,
            created = opened.created,
            refreshed = opened.refreshed.len(),
            "navigation resolved"
        );
        self.after_navigation();
        Ok(Some(opened))
    }

    /// Opens a location from outside the tree (sidebar, address bar,
    /// devices): the active branch's panel for it, or a new root branch.
    pub fn open_location(&mut self, location: Location) {
        self.workspace.open_root(location, self.defaults.inherit());
        self.after_navigation();
    }

    /// Breadcrumb navigation: activates the active branch's panel for
    /// `location`; for an ancestor of the root, inserts parent panels up to
    /// it; anything else opens a new root branch.
    pub fn open_ancestor(&mut self, location: Location) {
        if let Some(panel) = self.workspace.find_on_active_path(&location) {
            let _ = self.workspace.activate_panel(panel);
        } else if let Location::Path(target) = &location
            && let Some(&root) = self.workspace.active_path().first()
            && matches!(self.location(root), Some(Location::Path(r)) if r.starts_with(target))
        {
            let mut top = root;
            while self.location(top) != Some(&location) {
                match self.workspace.insert_parent(top) {
                    Ok(parent) => top = parent,
                    Err(_) => break,
                }
            }
        } else {
            self.workspace.open_root(location, self.defaults.inherit());
        }
        self.after_navigation();
    }

    // ── Presentation ────────────────────────────────────────────────────

    pub fn toggle_focus_mode(&mut self) -> bool {
        let on = !self.workspace.focus_mode();
        self.workspace.set_focus_mode(on);
        self.effects.push(Effect::Persist);
        on
    }

    pub fn set_focus_mode(&mut self, on: bool) {
        if self.workspace.set_focus_mode(on) {
            self.effects.push(Effect::Persist);
        }
    }

    pub fn set_retain_focus_on_branch_switch(&mut self, retain: bool) {
        self.workspace.set_retain_focus_on_branch_switch(retain);
    }

    /// The panels the presentation renders. They are loaded if needed and
    /// never evicted while shown.
    pub fn set_visible(&mut self, panels: &[PanelId]) {
        self.visible = panels.iter().copied().collect();
        for &panel in panels {
            self.ensure_listing(panel);
        }
        self.evict();
    }

    pub fn set_scroll(&mut self, panel: PanelId, scroll: Option<ScrollAnchor>) {
        if let Some(state) = self.workspace.state_mut(panel) {
            state.scroll = scroll;
        }
    }

    // ── Listings ────────────────────────────────────────────────────────

    /// Re-reads a panel (Refresh, or a debounced live update). A panel
    /// marked unavailable is retried.
    pub fn reload(&mut self, panel: PanelId) {
        let Some(location) = self.location(panel).cloned() else {
            return;
        };
        if !self.listings.contains_key(&panel) {
            self.ensure_listing(panel);
            return;
        }
        let generation = self.next_generation();
        let unavailable = self
            .workspace
            .panel(panel)
            .is_some_and(|p| p.unavailable().is_some());
        let listing = self.listings.get_mut(&panel).expect("checked");
        listing.load_generation = generation;
        if unavailable {
            // Retrying: watch again too, the old watch ended.
            listing.watch_generation = generation;
            self.effects.push(Effect::Watch {
                panel,
                location: location.clone(),
                generation,
            });
        }
        self.effects.push(Effect::Load {
            panel,
            location,
            generation,
        });
    }

    /// Reloads `panel` if `watch_generation` is still its current watch.
    pub fn reload_if_current(&mut self, panel: PanelId, watch_generation: u64) {
        if self
            .listings
            .get(&panel)
            .is_some_and(|l| l.watch_generation == watch_generation)
        {
            self.reload(panel);
        }
    }

    pub fn listing_loaded(
        &mut self,
        panel: PanelId,
        generation: u64,
        result: Result<Vec<FileEntry>, ListingError>,
    ) {
        let Some(listing) = self.listings.get_mut(&panel) else {
            return; // evicted or closed meanwhile
        };
        if listing.load_generation != generation {
            return; // superseded by a newer listing
        }
        match result {
            Ok(entries) => {
                listing.entries = entries;
                listing.loaded = true;
                self.workspace.mark_available(panel);
                self.rearrange(panel);
            }
            Err(err) => {
                tracing::debug!(%panel, error = %err, "listing failed");
                if err == ListingError::NotFound {
                    self.workspace
                        .mark_unavailable(panel, Unavailability::Missing);
                }
                let listing = self.listings.get_mut(&panel).expect("checked");
                listing.entries.clear();
                listing.shown.clear();
                listing.content = Content::Failed(err);
                listing.revision += 1;
            }
        }
    }

    /// A watched folder changed. Returns whether a (debounced) reload
    /// should follow; the shell then calls [`Self::reload_if_current`].
    pub fn directory_changed(
        &mut self,
        panel: PanelId,
        watch_generation: u64,
        change: DirectoryChange,
    ) -> bool {
        if !self
            .listings
            .get(&panel)
            .is_some_and(|l| l.watch_generation == watch_generation)
        {
            return false;
        }
        match change {
            DirectoryChange::Modified => true,
            DirectoryChange::Removed => {
                tracing::info!(%panel, "folder of panel was removed");
                self.mark_lost(&[panel], Unavailability::Missing);
                false
            }
        }
    }

    /// A device under `root` was removed, or is about to be ejected: panels
    /// on it stop watching and are marked unavailable. They are kept.
    pub fn detach_device(&mut self, root: &Path) -> usize {
        let panels = self.workspace.panels_within(root);
        self.mark_lost(&panels, Unavailability::DeviceRemoved);
        panels.len()
    }

    /// Selects `location` in `panel` as soon as it is listed.
    pub fn select_when_listed(&mut self, panel: PanelId, location: Location) {
        if let Some(listing) = self.listings.get_mut(&panel) {
            listing.pending_select = Some(location);
            self.apply_pending_select(panel);
        }
    }

    // ── Panel settings ──────────────────────────────────────────────────

    /// Changes `panel`'s search filter. Search belongs to the panel.
    pub fn set_query(&mut self, panel: PanelId, query: &str) {
        let Some(state) = self.workspace.state_mut(panel) else {
            return;
        };
        if state.options.query != query {
            state.options.query = query.to_owned();
            self.rearrange(panel);
            self.effects.push(Effect::Persist);
        }
    }

    pub fn toggle_hidden(&mut self, panel: PanelId) {
        self.change_settings(panel, |s| s.options.show_hidden = !s.options.show_hidden);
    }

    pub fn set_sort_key(&mut self, panel: PanelId, key: SortKey) {
        self.change_settings(panel, |s| s.options.sort.key = key);
    }

    pub fn toggle_sort_descending(&mut self, panel: PanelId) {
        self.change_settings(panel, |s| {
            s.options.sort.descending = !s.options.sort.descending
        });
    }

    pub fn set_view_mode(&mut self, panel: PanelId, mode: ViewMode) {
        self.change_settings(panel, |s| s.mode = mode);
    }

    // ── Cursor and selection ────────────────────────────────────────────

    /// Moves `panel`'s cursor by `delta` items (negative is up). With no
    /// cursor, moving down starts at the first item and up at the last.
    pub fn move_cursor(&mut self, panel: PanelId, delta: isize) -> Option<Location> {
        let len = self.listings.get(&panel)?.shown.len();
        if len == 0 {
            return None;
        }
        let current = self.cursor_index(panel);
        let target = match current {
            None if delta >= 0 => 0,
            None => len - 1,
            Some(i) => {
                let i = i as isize + delta;
                if self.wrap_cursor && delta.unsigned_abs() == 1 {
                    i.rem_euclid(len as isize) as usize
                } else {
                    i.clamp(0, len as isize - 1) as usize
                }
            }
        };
        self.set_cursor_index(panel, target)
    }

    pub fn cursor_to_first(&mut self, panel: PanelId) -> Option<Location> {
        self.set_cursor_index(panel, 0)
    }

    pub fn cursor_to_last(&mut self, panel: PanelId) -> Option<Location> {
        let len = self.listings.get(&panel)?.shown.len();
        self.set_cursor_index(panel, len.checked_sub(1)?)
    }

    pub fn set_cursor(&mut self, panel: PanelId, location: Option<Location>) {
        if let Some(state) = self.workspace.state_mut(panel)
            && state.cursor != location
        {
            state.cursor = location;
        }
    }

    pub fn cursor_index(&self, panel: PanelId) -> Option<usize> {
        let cursor = self.state(panel)?.cursor.as_ref()?;
        self.listings.get(&panel)?.position_of(cursor)
    }

    pub fn cursor_entry(&self, panel: PanelId) -> Option<&FileEntry> {
        let cursor = self.state(panel)?.cursor.as_ref()?;
        self.listings.get(&panel)?.entry(cursor)
    }

    /// Replaces `panel`'s selection (e.g. from a mouse click), keeping the
    /// selections of other panels.
    pub fn set_selection(&mut self, panel: PanelId, items: Vec<Location>) {
        if let Some(state) = self.workspace.state_mut(panel) {
            state.selection.replace(items);
        }
    }

    /// Adds or removes the cursor item from the selection.
    pub fn toggle_selection_at_cursor(&mut self, panel: PanelId) -> bool {
        let Some(state) = self.workspace.state_mut(panel) else {
            return false;
        };
        let Some(cursor) = state.cursor.clone() else {
            return false;
        };
        state.selection.set_anchor(Some(cursor.clone()));
        state.selection.toggle(cursor)
    }

    /// Starts a range selection at the cursor.
    pub fn start_range(&mut self, panel: PanelId) {
        if let Some(state) = self.workspace.state_mut(panel)
            && let Some(cursor) = state.cursor.clone()
        {
            state.selection.set_anchor(Some(cursor.clone()));
            state.selection.insert(cursor);
        }
    }

    /// Selects exactly the items between the range anchor and the cursor.
    pub fn extend_range(&mut self, panel: PanelId) {
        let (Some(listing), Some(state)) = (self.listings.get(&panel), self.state(panel)) else {
            return;
        };
        let (Some(anchor), Some(cursor)) = (state.selection.anchor(), state.cursor.as_ref()) else {
            return;
        };
        let (Some(a), Some(c)) = (listing.position_of(anchor), listing.position_of(cursor)) else {
            return;
        };
        let (from, to) = (a.min(c), a.max(c));
        let items: Vec<Location> = listing.shown[from..=to]
            .iter()
            .map(|e| e.location.clone())
            .collect();
        if let Some(state) = self.workspace.state_mut(panel) {
            state.selection.replace(items);
        }
    }

    pub fn select_all(&mut self, panel: PanelId) {
        let Some(listing) = self.listings.get(&panel) else {
            return;
        };
        let items: Vec<Location> = listing.shown.iter().map(|e| e.location.clone()).collect();
        self.set_selection(panel, items);
    }

    /// Clears the selection in every panel. Returns whether any existed.
    pub fn clear_selections(&mut self) -> bool {
        let panels: Vec<PanelId> = self.workspace.panels().map(|p| p.id()).collect();
        let mut any = false;
        for panel in panels {
            if let Some(state) = self.workspace.state_mut(panel)
                && !state.selection.is_empty()
            {
                state.selection.clear();
                any = true;
            }
        }
        any
    }

    /// The workspace-wide selection, grouped by panel in spatial order
    /// (branch by branch, root to tip).
    pub fn selection(&self) -> Vec<SelectionGroup> {
        let mut seen = HashSet::new();
        let mut groups = Vec::new();
        for branch in self.workspace.branches() {
            for panel in self.workspace.path(branch.id()) {
                if !seen.insert(panel) {
                    continue;
                }
                let p = self.workspace.panel(panel).expect("path panel");
                if !p.state.selection.is_empty() {
                    groups.push(SelectionGroup {
                        panel,
                        folder: p.location().clone(),
                        items: p.state.selection.items().to_vec(),
                    });
                }
            }
        }
        groups
    }

    /// What an operation acts on: the workspace-wide selection, or else the
    /// active panel's cursor item.
    pub fn targets(&self) -> Vec<SelectionGroup> {
        let selection = self.selection();
        if !selection.is_empty() {
            return selection;
        }
        let panel = self.active_panel();
        match self.cursor_entry(panel) {
            Some(entry) => vec![SelectionGroup {
                panel,
                folder: self.active_location().clone(),
                items: vec![entry.location.clone()],
            }],
            None => Vec::new(),
        }
    }

    // ── Internals ───────────────────────────────────────────────────────

    fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    fn after_navigation(&mut self) {
        let active = self.active_panel();
        self.ensure_listing(active);
        self.evict();
        self.effects.push(Effect::Persist);
    }

    fn ensure_listing(&mut self, panel: PanelId) {
        self.clock += 1;
        let now = self.clock;
        if let Some(listing) = self.listings.get_mut(&panel) {
            listing.last_used = now;
            return;
        }
        let Some(p) = self.workspace.panel(panel) else {
            return;
        };
        if let Some(reason) = p.unavailable() {
            // Nothing to read until the user retries (Refresh).
            let mut listing = PanelListing::new(0, now);
            listing.content = Content::Failed(match reason {
                Unavailability::DeviceRemoved => {
                    ListingError::Unavailable("The device was removed.".into())
                }
                Unavailability::Missing => ListingError::NotFound,
            });
            self.listings.insert(panel, listing);
            return;
        }
        let location = p.location().clone();
        let generation = self.next_generation();
        let mut listing = PanelListing::new(generation, now);
        listing.load_generation = generation;
        self.listings.insert(panel, listing);
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
        while self.listings.len() > self.capacity {
            let victim = self
                .listings
                .iter()
                .filter(|(id, _)| **id != active && !self.visible.contains(id))
                .min_by_key(|(_, l)| l.last_used)
                .map(|(id, _)| *id);
            let Some(victim) = victim else {
                break;
            };
            self.listings.remove(&victim);
            self.effects.push(Effect::Unwatch { panel: victim });
        }
    }

    fn mark_lost(&mut self, panels: &[PanelId], reason: Unavailability) {
        let mut affected = Vec::new();
        for &panel in panels {
            affected.extend(self.workspace.mark_unavailable(panel, reason));
        }
        let message = match reason {
            Unavailability::Missing => ListingError::NotFound,
            Unavailability::DeviceRemoved => {
                ListingError::Unavailable("The device was removed.".into())
            }
        };
        affected.sort();
        affected.dedup();
        for panel in affected {
            if let Some(listing) = self.listings.get_mut(&panel) {
                // Stop watching; any pending load is superseded.
                listing.watch_generation = 0;
                listing.load_generation = 0;
                listing.entries.clear();
                listing.shown.clear();
                listing.content = Content::Failed(message.clone());
                listing.revision += 1;
                self.effects.push(Effect::Unwatch { panel });
            }
        }
    }

    fn change_settings(&mut self, panel: PanelId, change: impl Fn(&mut PanelState)) {
        let Some(state) = self.workspace.state_mut(panel) else {
            return;
        };
        change(state);
        // D11: a panel's change also becomes the default for new panels.
        change(&mut self.defaults);
        self.rearrange(panel);
        self.effects.push(Effect::Persist);
    }

    /// Re-derives what `panel` shows, keeping its cursor on a valid item
    /// and dropping selections of files that are gone or filtered out.
    fn rearrange(&mut self, panel: PanelId) {
        let Some(state) = self.workspace.panel(panel).map(|p| p.state.clone()) else {
            return;
        };
        let Some(listing) = self.listings.get_mut(&panel) else {
            return;
        };
        if !listing.loaded {
            return; // first load still pending: nothing to arrange yet
        }
        let old_index = state.cursor.as_ref().and_then(|c| listing.position_of(c));
        listing.shown = arrange(&listing.entries, &state.options);
        listing.content = match (
            listing.shown.is_empty(),
            state.options.query.trim().is_empty(),
        ) {
            (false, _) => Content::Ready,
            (true, false) => Content::NoResults,
            (true, true) => Content::Empty,
        };
        listing.revision += 1;

        let existing: HashSet<&Location> = listing.entries.iter().map(|e| &e.location).collect();
        let cursor = match &state.cursor {
            Some(c) if listing.shown.iter().any(|e| e.location == *c) => Some(c.clone()),
            // Gone or filtered out: the item now at its old place, if any.
            Some(_) => old_index
                .map(|i| i.min(listing.shown.len().saturating_sub(1)))
                .and_then(|i| listing.shown.get(i))
                .map(|e| e.location.clone()),
            None => None,
        };
        // Only what the panel shows can stay selected: an operation must
        // never act on files hidden by a filter or that no longer exist.
        let shown: HashSet<&Location> = listing.shown.iter().map(|e| &e.location).collect();
        let state = self.workspace.state_mut(panel).expect("checked");
        state.cursor = cursor;
        state.selection.retain(|l| shown.contains(l));
        if let Some(scroll) = &state.scroll
            && !existing.contains(&scroll.item)
        {
            state.scroll = None;
        }
        self.apply_pending_select(panel);
    }

    fn apply_pending_select(&mut self, panel: PanelId) {
        let Some(listing) = self.listings.get_mut(&panel) else {
            return;
        };
        let Some(pending) = listing.pending_select.clone() else {
            return;
        };
        if listing.position_of(&pending).is_none() {
            return; // not listed yet; wait for the next reload
        }
        listing.pending_select = None;
        if let Some(state) = self.workspace.state_mut(panel) {
            state.cursor = Some(pending.clone());
            state.selection.replace([pending]);
        }
    }

    fn set_cursor_index(&mut self, panel: PanelId, index: usize) -> Option<Location> {
        let location = self
            .listings
            .get(&panel)?
            .shown
            .get(index)?
            .location
            .clone();
        self.set_cursor(panel, Some(location.clone()));
        Some(location)
    }
}
