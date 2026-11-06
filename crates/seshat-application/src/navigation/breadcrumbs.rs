use std::path::{Component, Path, PathBuf};

use seshat_domain::entities::Location;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breadcrumb {
    pub label: String,
    pub location: Location,
}

/// Splits a location into clickable segments. Paths under `home` start at a
/// "Home" segment instead of listing `/home/<user>`.
pub fn breadcrumbs(location: &Location, home: &Path) -> Vec<Breadcrumb> {
    match location {
        Location::Trash(rel) => {
            let mut crumbs = vec![Breadcrumb {
                label: "Trash".to_owned(),
                location: Location::trash_root(),
            }];
            let mut acc = PathBuf::new();
            for part in rel.iter() {
                acc.push(part);
                crumbs.push(Breadcrumb {
                    label: part.to_string_lossy().into_owned(),
                    location: Location::Trash(acc.clone()),
                });
            }
            crumbs
        }
        Location::Path(path) => {
            let (mut acc, rest, first_label) = match path.strip_prefix(home) {
                Ok(rest) if home.parent().is_some() => (home.to_path_buf(), rest, "Home"),
                _ => (
                    PathBuf::from("/"),
                    path.strip_prefix("/").unwrap_or(path),
                    "/",
                ),
            };
            let mut crumbs = vec![Breadcrumb {
                label: first_label.to_owned(),
                location: Location::Path(acc.clone()),
            }];
            for component in rest.components() {
                if let Component::Normal(part) = component {
                    acc.push(part);
                    crumbs.push(Breadcrumb {
                        label: part.to_string_lossy().into_owned(),
                        location: Location::Path(acc.clone()),
                    });
                }
            }
            crumbs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(crumbs: &[Breadcrumb]) -> Vec<&str> {
        crumbs.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn home_paths_start_at_home() {
        let crumbs = breadcrumbs(
            &Location::Path("/home/u/Docs/x".into()),
            Path::new("/home/u"),
        );
        assert_eq!(labels(&crumbs), ["Home", "Docs", "x"]);
        assert_eq!(crumbs[1].location, Location::Path("/home/u/Docs".into()));
    }

    #[test]
    fn other_paths_start_at_root() {
        let crumbs = breadcrumbs(&Location::Path("/run/media".into()), Path::new("/home/u"));
        assert_eq!(labels(&crumbs), ["/", "run", "media"]);
        assert_eq!(crumbs[0].location, Location::Path("/".into()));
    }

    #[test]
    fn root_is_a_single_crumb() {
        let crumbs = breadcrumbs(&Location::Path("/".into()), Path::new("/home/u"));
        assert_eq!(labels(&crumbs), ["/"]);
    }

    #[test]
    fn home_prefix_must_match_whole_components() {
        let crumbs = breadcrumbs(&Location::Path("/home/user2".into()), Path::new("/home/u"));
        assert_eq!(labels(&crumbs), ["/", "home", "user2"]);
    }

    #[test]
    fn trash_paths() {
        let crumbs = breadcrumbs(&Location::Trash("old/sub".into()), Path::new("/home/u"));
        assert_eq!(labels(&crumbs), ["Trash", "old", "sub"]);
        assert_eq!(crumbs[1].location, Location::Trash("old".into()));
    }
}
