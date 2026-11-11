//! The panels' listing cache: which panels have a listing in memory (and
//! are watched), least-recently-used eviction beyond a capacity, and the
//! generations that tell current loads and watches from stale ones
//! (decision D13).
//!
//! It does no I/O and produces no effects: it reports what it evicted or
//! removed, and the [`WorkspaceService`](super::WorkspaceService) turns that
//! into `Unwatch` effects.

use std::collections::{HashMap, HashSet};

use seshat_domain::workspace::PanelId;

use super::listing::{Content, PanelListing};
use crate::ports::ListingError;

#[derive(Debug)]
pub(super) struct ListingCache {
    listings: HashMap<PanelId, PanelListing>,
    /// Listings kept at most (visible and active panels may exceed it).
    capacity: usize,
    /// Panels the presentation is currently rendering; never evicted.
    visible: HashSet<PanelId>,
    /// Logical time of the last use, for least-recently-used eviction.
    clock: u64,
    /// Numbers loads and watches, so late results can be recognised.
    generation: u64,
}

impl ListingCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            listings: HashMap::new(),
            capacity: capacity.max(1),
            visible: HashSet::new(),
            clock: 0,
            generation: 0,
        }
    }

    pub(super) fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
    }

    pub(super) fn get(&self, panel: &PanelId) -> Option<&PanelListing> {
        self.listings.get(panel)
    }

    pub(super) fn get_mut(&mut self, panel: &PanelId) -> Option<&mut PanelListing> {
        self.listings.get_mut(panel)
    }

    pub(super) fn contains_key(&self, panel: &PanelId) -> bool {
        self.listings.contains_key(panel)
    }

    pub(super) fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Marks `panel`'s listing as just used. Returns whether it is cached.
    pub(super) fn touch(&mut self, panel: PanelId) -> bool {
        let now = self.tick();
        match self.listings.get_mut(&panel) {
            Some(listing) => {
                listing.last_used = now;
                true
            }
            None => false,
        }
    }

    /// Starts a listing for `panel` that is being loaded and watched. The
    /// load and the watch share the returned generation.
    pub(super) fn start_loading(&mut self, panel: PanelId) -> u64 {
        let generation = self.next_generation();
        let mut listing = PanelListing::new(generation, self.tick());
        listing.load_generation = generation;
        self.listings.insert(panel, listing);
        generation
    }

    /// A listing that shows `error` without reading anything (an
    /// unavailable panel, until the user retries).
    pub(super) fn insert_failed(&mut self, panel: PanelId, error: ListingError) {
        let mut listing = PanelListing::new(0, self.tick());
        listing.content = Content::Failed(error);
        self.listings.insert(panel, listing);
    }

    /// Drops `panel`'s listing (closed, pruned, renamed or replaced).
    /// Returns whether it was cached, and so watched.
    pub(super) fn remove(&mut self, panel: PanelId) -> bool {
        self.visible.remove(&panel);
        self.listings.remove(&panel).is_some()
    }

    pub(super) fn set_visible(&mut self, panels: &[PanelId]) {
        self.visible = panels.iter().copied().collect();
    }

    /// Drops the least recently used listings beyond capacity, never
    /// `keep` (the active panel) or a visible one. Returns the panels whose
    /// listings were dropped; their logical state is untouched.
    pub(super) fn evict(&mut self, keep: PanelId) -> Vec<PanelId> {
        let mut evicted = Vec::new();
        while self.listings.len() > self.capacity {
            let victim = self
                .listings
                .iter()
                .filter(|(id, _)| **id != keep && !self.visible.contains(id))
                .min_by_key(|(_, l)| l.last_used)
                .map(|(id, _)| *id);
            let Some(victim) = victim else {
                break;
            };
            self.listings.remove(&victim);
            evicted.push(victim);
        }
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(raw: u64) -> PanelId {
        PanelId::from_raw(raw)
    }

    fn cache_with(capacity: usize, panels: &[u64]) -> ListingCache {
        let mut cache = ListingCache::new(capacity);
        for &raw in panels {
            cache.start_loading(p(raw));
        }
        cache
    }

    #[test]
    fn the_least_recently_used_listing_goes_first() {
        let mut cache = cache_with(2, &[1, 2, 3]);
        assert_eq!(cache.evict(p(3)), [p(1)]);
        assert!(!cache.contains_key(&p(1)));

        // Using 2 makes 3 the oldest.
        let mut cache = cache_with(2, &[1, 2, 3]);
        assert!(cache.touch(p(1)));
        assert!(cache.touch(p(2)));
        assert_eq!(cache.evict(p(2)), [p(3)]);
    }

    #[test]
    fn the_active_and_visible_panels_are_never_evicted() {
        let mut cache = cache_with(1, &[1, 2, 3]);
        cache.set_visible(&[p(1), p(2)]);
        assert_eq!(cache.evict(p(3)), Vec::<PanelId>::new());
        assert!(
            cache.contains_key(&p(1)) && cache.contains_key(&p(2)) && cache.contains_key(&p(3))
        );
        cache.set_visible(&[]);
        assert_eq!(cache.evict(p(3)), [p(1), p(2)]);
    }

    #[test]
    fn capacity_is_at_least_one() {
        let mut cache = cache_with(0, &[1, 2]);
        assert_eq!(cache.evict(p(2)), [p(1)]);
        cache.set_capacity(0);
        cache.start_loading(p(3));
        assert_eq!(cache.evict(p(3)), [p(2)]);
    }

    #[test]
    fn removing_a_listing_also_forgets_that_it_was_visible() {
        let mut cache = cache_with(1, &[1, 2]);
        cache.set_visible(&[p(1)]);
        assert!(cache.remove(p(1)));
        assert!(!cache.remove(p(1)), "already gone");
        cache.start_loading(p(1));
        // 1 is no longer protected as visible.
        assert_eq!(cache.evict(p(2)), [p(1)]);
    }

    #[test]
    fn loads_get_increasing_generations_and_failures_none() {
        let mut cache = ListingCache::new(4);
        let first = cache.start_loading(p(1));
        let second = cache.start_loading(p(2));
        assert!(second > first);
        let listing = cache.get(&p(2)).unwrap();
        assert_eq!(listing.watch_generation(), second);
        assert_eq!(listing.load_generation, second);
        assert!(cache.next_generation() > second);

        cache.insert_failed(p(3), ListingError::NotFound);
        let failed = cache.get(&p(3)).unwrap();
        assert_eq!(failed.watch_generation(), 0, "nothing is watched");
        assert_eq!(*failed.content(), Content::Failed(ListingError::NotFound));
        assert!(!cache.touch(p(9)), "not cached");
    }
}
