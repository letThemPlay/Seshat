use std::path::{Component, Path, PathBuf};

use seshat_domain::entities::Location;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddressError {
    #[error("enter a location")]
    Empty,
    #[error("“{0}” locations are not supported")]
    UnsupportedScheme(String),
    #[error("relative paths need a folder to be relative to")]
    NoBase,
}

/// Parses text typed into the address bar.
///
/// Accepts absolute paths, `~` / `~/…`, `trash:` / `trash:///…`,
/// `file:///…` and paths relative to `current`. `.` and `..` are resolved
/// lexically, as the user typed them, without touching the filesystem.
pub fn parse_address(
    input: &str,
    home: &Path,
    current: Option<&Location>,
) -> Result<Location, AddressError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AddressError::Empty);
    }

    if let Some(rest) = input.strip_prefix("trash:") {
        return Ok(Location::Trash(normalize(
            Path::new(rest.trim_start_matches('/')),
            true,
        )));
    }
    if let Some(rest) = input.strip_prefix("file://") {
        return Ok(Location::Path(normalize(Path::new(rest), false)));
    }
    if let Some((scheme, _)) = input.split_once("://") {
        return Err(AddressError::UnsupportedScheme(scheme.to_owned()));
    }

    let path = if input == "~" {
        home.to_path_buf()
    } else if let Some(rest) = input.strip_prefix("~/") {
        home.join(rest)
    } else if input.starts_with('/') {
        PathBuf::from(input)
    } else {
        match current {
            Some(Location::Path(base)) => base.join(input),
            Some(Location::Trash(base)) => {
                return Ok(Location::Trash(normalize(&base.join(input), true)));
            }
            None => return Err(AddressError::NoBase),
        }
    };
    Ok(Location::Path(normalize(&path, false)))
}

/// Lexically resolves `.` and `..`. For relative (Trash) paths `..` cannot
/// climb above the root.
fn normalize(path: &Path, relative: bool) -> PathBuf {
    let mut out = if relative {
        PathBuf::new()
    } else {
        PathBuf::from("/")
    };
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/u";

    fn parse(input: &str) -> Result<Location, AddressError> {
        parse_address(
            input,
            Path::new(HOME),
            Some(&Location::Path("/srv/data".into())),
        )
    }

    fn path(s: &str) -> Result<Location, AddressError> {
        Ok(Location::Path(s.into()))
    }

    #[test]
    fn absolute_and_home() {
        assert_eq!(parse("/etc"), path("/etc"));
        assert_eq!(parse("~"), path(HOME));
        assert_eq!(parse("~/Music/"), path("/home/u/Music"));
        assert_eq!(parse("  /tmp  "), path("/tmp"));
    }

    #[test]
    fn dots_are_resolved_lexically() {
        assert_eq!(parse("/a/./b/../c"), path("/a/c"));
        assert_eq!(parse("/../.."), path("/"));
        assert_eq!(parse(".."), path("/srv"));
        assert_eq!(parse("sub/x"), path("/srv/data/sub/x"));
    }

    #[test]
    fn file_uris() {
        assert_eq!(parse("file:///var/log"), path("/var/log"));
    }

    #[test]
    fn trash_addresses() {
        assert_eq!(parse("trash:"), Ok(Location::trash_root()));
        assert_eq!(parse("trash:///"), Ok(Location::trash_root()));
        assert_eq!(parse("trash:///old/x"), Ok(Location::Trash("old/x".into())));
        assert_eq!(parse("trash:///../.."), Ok(Location::trash_root()));
    }

    #[test]
    fn relative_inside_trash() {
        let current = Location::Trash("old".into());
        let parsed = parse_address("sub", Path::new(HOME), Some(&current));
        assert_eq!(parsed, Ok(Location::Trash("old/sub".into())));
    }

    #[test]
    fn errors() {
        assert_eq!(parse(""), Err(AddressError::Empty));
        assert_eq!(
            parse("smb://host/share"),
            Err(AddressError::UnsupportedScheme("smb".into()))
        );
        assert_eq!(
            parse_address("rel", Path::new(HOME), None),
            Err(AddressError::NoBase)
        );
    }
}
