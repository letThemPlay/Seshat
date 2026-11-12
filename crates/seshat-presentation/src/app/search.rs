//! The panel search bar (`/`, Ctrl+F): starting and closing a search,
//! filtering the active panel as the user types, and showing each panel's
//! own query when another panel becomes active (search belongs to the
//! panel, spatial spec §11).

use gtk::prelude::*;

use seshat_application::workspace::WorkspaceService;

#[derive(Debug)]
pub enum SearchMsg {
    /// The search text changed (typed, or set by [`SearchController::sync`]).
    Changed(String),
    /// Escape: leave an active search.
    Close,
    /// Return or Down in the search box: go to the results.
    FocusResults,
}

/// What the window must do after a search message.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SearchOutcome {
    Stay,
    /// Move keyboard focus to the active panel's cursor.
    FocusFiles,
}

pub(super) struct SearchController {
    bar: gtk::SearchBar,
    entry: gtk::SearchEntry,
    /// Text this controller put into the entry, so the change it causes is
    /// not taken for typing.
    echo: Option<String>,
}

impl SearchController {
    pub(super) fn new(bar: gtk::SearchBar, entry: gtk::SearchEntry) -> Self {
        Self {
            bar,
            entry,
            echo: None,
        }
    }

    pub(super) fn update(
        &mut self,
        msg: SearchMsg,
        service: &mut WorkspaceService,
    ) -> SearchOutcome {
        match msg {
            SearchMsg::Changed(query) => {
                if self.echo.take().as_deref() != Some(query.as_str()) {
                    let active = service.active_panel();
                    service.set_query(active, &query);
                }
            }
            SearchMsg::Close => {
                self.close();
            }
            SearchMsg::FocusResults => {
                let active = service.active_panel();
                if service.state(active).is_some_and(|s| s.cursor.is_none()) {
                    service.cursor_to_first(active);
                }
                return SearchOutcome::FocusFiles;
            }
        }
        SearchOutcome::Stay
    }

    /// `/` or Ctrl+F. Like Vim's `/`, this starts a new search: a panel's
    /// remembered query (restored with the panel) is selected, so typing
    /// replaces it and an arrow key keeps it for editing.
    pub(super) fn start(&self) {
        self.bar.set_search_mode(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    /// Leaves an active search. Returns whether one was open.
    pub(super) fn close(&self) -> bool {
        let open = self.bar.is_search_mode();
        if open {
            self.bar.set_search_mode(false);
        }
        open
    }

    /// Shows the active panel's own search in the search bar.
    pub(super) fn sync(&mut self, query: &str) {
        if self.entry.text() != query {
            self.echo = Some(query.to_owned());
            self.entry.set_text(query);
        }
        let show = !query.is_empty();
        if self.bar.is_search_mode() != show {
            if !show {
                self.echo = Some(String::new());
            }
            self.bar.set_search_mode(show);
        }
    }
}
