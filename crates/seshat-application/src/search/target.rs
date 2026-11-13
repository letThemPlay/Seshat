//! What the text typed into Fuzzy Find searches (§6, decisions F10, F14).
//!
//! A query naming a folder (`/…`, `~/…`, [`PathQuery`]) searches that
//! folder, and the scopes don't apply while it does. Any other query
//! searches the scope the user chose. Kept out of the UI so the rules are
//! the same wherever a search is started from.

use std::path::Path;

use seshat_domain::search::SearchScope;
use seshat_domain::workspace::Workspace;

use super::path_query::PathQuery;
use super::scope::{ScopeError, SearchRoot, roots};
use crate::preferences::ScopeChoice;

/// The folders a query searches and the text matched in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTarget {
    /// The chosen scope, or `None` while the query names a folder (F10).
    pub scope: Option<ScopeChoice>,
    /// Never empty.
    pub roots: Vec<SearchRoot>,
    /// The text to match: the query, or what follows a named folder.
    pub pattern: String,
}

impl SearchTarget {
    /// What `text` searches with `chosen` as the scope. `home` is what `~`
    /// stands for.
    pub fn resolve(
        text: &str,
        chosen: ScopeChoice,
        workspace: &Workspace,
        home: &Path,
    ) -> Result<Self, ScopeError> {
        let (scope, choice, pattern) = match PathQuery::parse(text, home) {
            Some(query) => (SearchScope::AbsolutePath(query.folder), None, query.pattern),
            None => (chosen.scope(), Some(chosen), text.to_owned()),
        };
        Ok(Self {
            scope: choice,
            roots: roots(&scope, workspace)?,
            pattern,
        })
    }

    /// Whether picking another scope changes anything: not while the query
    /// names a folder (F14).
    pub fn accepts_scope(&self) -> bool {
        self.scope.is_some()
    }

    /// Whether results name the branch they were found in: only when every
    /// branch is searched (§6.3).
    pub fn shows_branches(&self) -> bool {
        self.scope == Some(ScopeChoice::EntireWorkspace)
    }

    /// The folder Enter opens when the query names a folder and nothing
    /// follows it, as the address bar would (F10, D3).
    pub fn folder_to_open(&self) -> Option<&Path> {
        if self.scope.is_some() || !self.pattern.trim().is_empty() {
            return None;
        }
        self.roots.first().map(|root| root.path.as_path())
    }
}

#[cfg(test)]
mod tests {
    use seshat_domain::entities::Location;
    use seshat_domain::workspace::PanelState;

    use super::*;

    const HOME: &str = "/h";

    fn workspace() -> Workspace {
        let mut ws = Workspace::new(Location::Path(HOME.into()), PanelState::default());
        let home = ws.active_panel();
        ws.open_child(home, Location::Path("/h/Docs".into()), None)
            .unwrap();
        ws
    }

    fn resolve(text: &str, chosen: ScopeChoice) -> SearchTarget {
        SearchTarget::resolve(text, chosen, &workspace(), Path::new(HOME)).unwrap()
    }

    fn paths(target: &SearchTarget) -> Vec<&Path> {
        target.roots.iter().map(|r| r.path.as_path()).collect()
    }

    #[test]
    fn an_ordinary_query_searches_the_chosen_scope() {
        let target = resolve("notes", ScopeChoice::CurrentDirectory);
        assert_eq!(target.scope, Some(ScopeChoice::CurrentDirectory));
        assert_eq!(paths(&target), [Path::new("/h/Docs")]);
        assert_eq!(target.pattern, "notes");
        assert!(target.accepts_scope());
        assert_eq!(target.folder_to_open(), None);

        let branch = resolve("notes", ScopeChoice::CurrentBranch);
        assert_eq!(paths(&branch), [Path::new("/h")]);
    }

    #[test]
    fn a_typed_folder_overrides_the_scope() {
        let target = resolve("~/Music/son", ScopeChoice::EntireWorkspace);
        assert_eq!(target.scope, None);
        assert_eq!(paths(&target), [Path::new("/h/Music")]);
        assert_eq!(target.roots[0].origin, None);
        assert_eq!(target.pattern, "son");
        assert!(!target.accepts_scope(), "scopes don't apply (F14)");
        assert!(!target.shows_branches());
        assert_eq!(target.folder_to_open(), None, "there is something to find");
    }

    #[test]
    fn enter_opens_a_typed_folder_with_nothing_after_it() {
        assert_eq!(
            resolve("/srv/", ScopeChoice::CurrentDirectory).folder_to_open(),
            Some(Path::new("/srv"))
        );
        assert_eq!(
            resolve("~", ScopeChoice::CurrentDirectory).folder_to_open(),
            Some(Path::new(HOME))
        );
        assert_eq!(
            resolve("", ScopeChoice::CurrentDirectory).folder_to_open(),
            None,
            "an empty ordinary query opens nothing"
        );
    }

    #[test]
    fn only_the_entire_workspace_names_branches() {
        assert!(resolve("x", ScopeChoice::EntireWorkspace).shows_branches());
        assert!(!resolve("x", ScopeChoice::CurrentBranch).shows_branches());
        assert!(!resolve("x", ScopeChoice::CurrentDirectory).shows_branches());
    }

    #[test]
    fn the_trash_cannot_be_searched() {
        let mut ws = workspace();
        ws.open_root(Location::trash_root(), PanelState::default());
        assert_eq!(
            SearchTarget::resolve("x", ScopeChoice::CurrentDirectory, &ws, Path::new(HOME)),
            Err(ScopeError::InTrash)
        );
        assert!(
            SearchTarget::resolve(
                "/tmp/x",
                ScopeChoice::CurrentDirectory,
                &ws,
                Path::new(HOME)
            )
            .is_ok(),
            "a typed folder can still be searched from the Trash"
        );
    }
}
