//! Cursors and the workspace-wide selection, and the targets file
//! operations act on.

use std::collections::HashSet;

use seshat_domain::entities::{FileEntry, Location};
use seshat_domain::workspace::PanelId;

use super::{SelectionGroup, WorkspaceService};

impl WorkspaceService {
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
