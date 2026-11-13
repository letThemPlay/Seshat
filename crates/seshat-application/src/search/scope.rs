//! Which folders a [`SearchScope`] covers in the current workspace (§6).

use std::path::PathBuf;

use seshat_domain::entities::Location;
use seshat_domain::search::{SearchOrigin, SearchScope};
use seshat_domain::workspace::{PanelId, Workspace};
use thiserror::Error;

/// A folder to search, and the panel and branch it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchRoot {
    pub path: PathBuf,
    /// `None` for a typed absolute path that no panel shows.
    pub origin: Option<SearchOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScopeError {
    /// The Trash is not a single folder on disk.
    #[error("the Trash can't be searched")]
    InTrash,
    #[error("“{}” is not an absolute path", .0.display())]
    NotAbsolute(PathBuf),
}

/// The folders `scope` covers. Never empty on success.
///
/// - **Current Directory**: the active panel's folder.
/// - **Current Branch**: the active branch's root folder (decision F2).
///   Every panel of a branch lies inside it, so it covers the branch.
/// - **Entire Workspace**: the root folder of every branch, in branch
///   order. A folder shared by several branches, or lying inside another
///   root, is searched once (§6.3). Branches rooted in the Trash are left
///   out.
/// - **Absolute Path**: that folder. Whether it exists is reported by the
///   walk.
pub fn roots(scope: &SearchScope, workspace: &Workspace) -> Result<Vec<SearchRoot>, ScopeError> {
    let branch = workspace.active_branch().id();
    let single = |panel| {
        root_of(workspace, branch, panel)
            .map(|root| vec![root])
            .ok_or(ScopeError::InTrash)
    };
    match scope {
        SearchScope::CurrentDirectory => single(workspace.active_panel()),
        SearchScope::CurrentBranch => single(workspace.path(branch)[0]),
        SearchScope::EntireWorkspace => {
            let mut roots: Vec<SearchRoot> = Vec::new();
            for branch in workspace.branches() {
                let panel = workspace.path(branch.id())[0];
                if let Some(root) = root_of(workspace, branch.id(), panel) {
                    roots.push(root);
                }
            }
            // A root inside another (or equal to it) adds nothing. The
            // earlier branch keeps an equal root.
            let all = roots.clone();
            let mut kept: Vec<SearchRoot> = Vec::new();
            for (i, root) in all.iter().enumerate() {
                let covered = all.iter().enumerate().any(|(j, other)| {
                    j != i
                        && root.path.starts_with(&other.path)
                        && (root.path != other.path || j < i)
                });
                if !covered {
                    kept.push(root.clone());
                }
            }
            if kept.is_empty() {
                return Err(ScopeError::InTrash);
            }
            Ok(kept)
        }
        SearchScope::AbsolutePath(path) => {
            if !path.is_absolute() {
                return Err(ScopeError::NotAbsolute(path.clone()));
            }
            Ok(vec![SearchRoot {
                path: path.clone(),
                origin: None,
            }])
        }
    }
}

fn root_of(
    workspace: &Workspace,
    branch: seshat_domain::workspace::BranchId,
    panel: PanelId,
) -> Option<SearchRoot> {
    match workspace.panel(panel)?.location() {
        Location::Path(path) => Some(SearchRoot {
            path: path.clone(),
            origin: Some(SearchOrigin { branch, panel }),
        }),
        Location::Trash(_) => None,
    }
}
