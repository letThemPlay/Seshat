//! Opening and revealing Fuzzy Find results (Spec 4 §9) with the
//! workspace's own navigation: no separate model of panels or branches.
//!
//! A result remembers the panel it was found under ([`SearchOrigin`]). A
//! folder result is opened along the path from that panel, one folder at
//! a time ([`Workspace::open_path`], decision F5), so existing panels are
//! reused and a branch is only started where continuing would replace a
//! path. Results with no panel to start from (a typed absolute path, or a
//! panel closed meanwhile) open like a location typed in the address bar
//! (decision D3). Files are opened in their applications and the
//! workspace is left as it is.
//!
//! [`SearchOrigin`]: seshat_domain::search::SearchOrigin
//! [`Workspace::open_path`]: seshat_domain::workspace::Workspace::open_path

use seshat_domain::entities::Location;
use seshat_domain::search::{SearchOrigin, SearchResult};
use seshat_domain::workspace::PanelId;

use super::{Effect, WorkspaceService};

impl WorkspaceService {
    /// Enter on a result: a folder (or a link to one) is opened in the
    /// workspace, a file in its default application (§9.1, §9.2).
    pub fn open_result(&mut self, result: &SearchResult) {
        if !result.kind.is_browsable() {
            self.effects
                .push(Effect::Launch(vec![result.location.clone()]));
            return;
        }
        self.open_below(result.origin, result.location.clone());
    }

    /// Alt+Enter on a result: opens the folder it is in and puts the cursor
    /// on it (§9.4). The panel's selection is left alone (decision F9).
    pub fn reveal_result(&mut self, result: &SearchResult) {
        let Some(parent) = result.location.parent() else {
            return; // the filesystem root has no folder to show it in
        };
        self.open_below(result.origin, parent);
        let panel = self.active_panel();
        self.cursor_when_listed(panel, result.location.clone());
    }

    /// Opens `target` along the path from the origin panel, or, without a
    /// usable origin, as a location from outside the tree.
    fn open_below(&mut self, origin: Option<SearchOrigin>, target: Location) {
        let Some((from, steps)) = origin.and_then(|o| self.steps_below(o.panel, &target)) else {
            self.open_location(target);
            return;
        };
        match self.workspace.open_path(from, steps) {
            Ok(opened) => {
                self.forget_panels(&opened.refreshed);
                tracing::debug!(
                    resolution = ?opened.resolution,
                    branch = %opened.branch,
                    panel = %opened.panel,
                    new_branch = opened.new_branch,
                    "search result opened"
                );
                self.after_navigation();
            }
            // Checked by `steps_below`; never expected.
            Err(err) => {
                tracing::warn!(error = %err, "search result not opened");
                self.open_location(target);
            }
        }
    }

    /// The folders between `panel`'s folder and `target`, top to bottom,
    /// when `panel` still exists and `target` lies at or below its folder.
    fn steps_below(&self, panel: PanelId, target: &Location) -> Option<(PanelId, Vec<Location>)> {
        let (Location::Path(base), Location::Path(goal)) = (self.location(panel)?, target) else {
            return None;
        };
        let rest = goal.strip_prefix(base).ok()?;
        let mut current = base.clone();
        let steps = rest
            .components()
            .map(|component| {
                current.push(component);
                Location::Path(current.clone())
            })
            .collect();
        Some((panel, steps))
    }
}
