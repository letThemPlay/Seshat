//! Panel listings: loading and watching folders, live updates, folders and
//! devices that disappear, and deriving what each panel shows.

use std::collections::HashSet;
use std::path::Path;

use seshat_domain::entities::{FileEntry, Location};
use seshat_domain::workspace::{PanelId, Unavailability};

use super::{Effect, WorkspaceService};
use crate::ports::{DirectoryChange, ListingError};
use crate::view::arrange;
use crate::workspace::listing::Content;

impl WorkspaceService {
    /// The panels the presentation renders. They are loaded if needed and
    /// never evicted while shown.
    pub fn set_visible(&mut self, panels: &[PanelId]) {
        self.listings.set_visible(panels);
        for &panel in panels {
            self.ensure_listing(panel);
        }
        self.evict();
    }

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
                self.listings.get_mut(&panel).expect("checked").fail(err);
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

    fn mark_lost(&mut self, panels: &[PanelId], reason: Unavailability) {
        let mut affected = Vec::new();
        for &panel in panels {
            affected.extend(self.workspace.mark_unavailable(panel, reason));
        }
        let message = ListingError::from(reason);
        affected.sort();
        affected.dedup();
        for panel in affected {
            if let Some(listing) = self.listings.get_mut(&panel) {
                listing.stop_watching();
                listing.fail(message.clone());
                self.effects.push(Effect::Unwatch { panel });
            }
        }
    }

    /// Re-derives what `panel` shows, keeping its cursor on a valid item
    /// and dropping selections of files that are gone or filtered out.
    pub(super) fn rearrange(&mut self, panel: PanelId) {
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
}
