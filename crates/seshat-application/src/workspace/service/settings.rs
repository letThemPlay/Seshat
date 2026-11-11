//! Each panel's own settings: search filter, hidden files, sort order,
//! view mode and scroll position.

use seshat_domain::view::{SortKey, ViewMode};
use seshat_domain::workspace::{PanelId, PanelState, ScrollAnchor};

use super::{Effect, WorkspaceService};

impl WorkspaceService {
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

    pub fn set_scroll(&mut self, panel: PanelId, scroll: Option<ScrollAnchor>) {
        if let Some(state) = self.workspace.state_mut(panel) {
            state.scroll = scroll;
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
}
