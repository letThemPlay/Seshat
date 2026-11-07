use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A sibling of `path` that does not exist yet: `name (2).ext`,
/// `name (3).ext`, … The extension is kept for files; folders and
/// dotfiles get the counter at the end.
pub fn unique_path(path: &Path, is_dir: bool, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("/"));
    let name = path.file_name().map(OsString::from).unwrap_or_default();
    let (stem, ext) = split_name(&name, is_dir);
    (2u64..)
        .map(|n| {
            let mut candidate = stem.clone();
            candidate.push(format!(" ({n})"));
            candidate.push(&ext);
            parent.join(candidate)
        })
        .find(|candidate| !exists(candidate))
        .expect("an unused name exists")
}

/// Splits `report.tar.gz` into (`report`, `.tar.gz`) and `photo.png` into
/// (`photo`, `.png`). Folders and names starting with a dot have no extension.
fn split_name(name: &OsString, is_dir: bool) -> (OsString, OsString) {
    let bytes = name.as_encoded_bytes();
    if is_dir || bytes.first() == Some(&b'.') {
        return (name.clone(), OsString::new());
    }
    let lossy = name.to_string_lossy();
    let split = if lossy.ends_with(".tar.gz")
        || lossy.ends_with(".tar.xz")
        || lossy.ends_with(".tar.bz2")
        || lossy.ends_with(".tar.zst")
    {
        lossy.rfind(".tar.")
    } else {
        lossy.rfind('.')
    };
    match split {
        Some(i) if i > 0 && lossy.is_char_boundary(i) && lossy.len() == bytes.len() => {
            (OsString::from(&lossy[..i]), OsString::from(&lossy[i..]))
        }
        _ => (name.clone(), OsString::new()),
    }
}

/// Name for a temporary sibling file used for atomic replacement.
pub fn temp_sibling(path: &Path, token: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.seshat-part-{token}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn unique(path: &str, is_dir: bool, taken: &[&str]) -> String {
        let taken: HashSet<PathBuf> = taken.iter().map(PathBuf::from).collect();
        unique_path(Path::new(path), is_dir, |p| taken.contains(p))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn keeps_extensions() {
        assert_eq!(unique("/d/photo.png", false, &[]), "/d/photo (2).png");
        assert_eq!(unique("/d/a.tar.gz", false, &[]), "/d/a (2).tar.gz");
        assert_eq!(
            unique(
                "/d/photo.png",
                false,
                &["/d/photo (2).png", "/d/photo (3).png"]
            ),
            "/d/photo (4).png"
        );
    }

    #[test]
    fn folders_and_dotfiles_have_no_extension() {
        assert_eq!(unique("/d/v1.2", true, &[]), "/d/v1.2 (2)");
        assert_eq!(unique("/d/.bashrc", false, &[]), "/d/.bashrc (2)");
        assert_eq!(unique("/d/Makefile", false, &[]), "/d/Makefile (2)");
    }

    #[test]
    fn non_utf8_names_keep_their_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
        let path = Path::new("/d").join(name);
        let result = unique_path(&path, false, |_| false);
        assert_eq!(result.file_name().unwrap().as_bytes(), b"caf\xe9.txt (2)");
    }

    #[test]
    fn temp_sibling_is_hidden_in_same_dir() {
        assert_eq!(
            temp_sibling(Path::new("/d/a.txt"), "x1"),
            PathBuf::from("/d/.a.txt.seshat-part-x1")
        );
    }
}
