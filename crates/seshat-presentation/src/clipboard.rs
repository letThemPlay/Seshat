//! File copy/cut/paste through the Wayland clipboard.
//!
//! Interoperates with other file managers:
//! - `x-special/gnome-copied-files` (GNOME Files, Nemo, …): `copy|cut\nURI…`
//! - `application/x-kde-cutselection` (Dolphin): `1` marks a cut
//! - `text/uri-list` via `GdkFileList` (everything else; treated as copy)

use std::path::PathBuf;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use seshat_application::workspace::TransferKind;

const GNOME_FORMAT: &str = "x-special/gnome-copied-files";
const KDE_CUT_FORMAT: &str = "application/x-kde-cutselection";

/// Files on the clipboard, and whether pasting copies them or moves them
/// (they were cut).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    pub kind: TransferKind,
}

/// Puts `paths` on the clipboard in all the formats above, and as plain
/// text.
pub fn set_files(display: &gdk::Display, paths: &[PathBuf], kind: TransferKind) {
    let files: Vec<gio::File> = paths.iter().map(gio::File::for_path).collect();
    let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
    let word = match kind {
        TransferKind::Copy => "copy",
        TransferKind::Move => "cut",
    };
    let gnome = format!("{word}\n{}", uris.join("\n"));
    let text = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");

    let mut providers = vec![
        gdk::ContentProvider::for_bytes(GNOME_FORMAT, &glib::Bytes::from_owned(gnome.into_bytes())),
        gdk::ContentProvider::for_value(&gdk::FileList::from_array(&files).to_value()),
        gdk::ContentProvider::for_bytes(
            "text/plain;charset=utf-8",
            &glib::Bytes::from_owned(text.into_bytes()),
        ),
    ];
    if kind == TransferKind::Move {
        providers.push(gdk::ContentProvider::for_bytes(
            KDE_CUT_FORMAT,
            &glib::Bytes::from_static(b"1"),
        ));
    }
    tracing::debug!(items = paths.len(), ?kind, "files put on the clipboard");
    if let Err(err) = display
        .clipboard()
        .set_content(Some(&gdk::ContentProvider::new_union(&providers)))
    {
        tracing::warn!(error = %err, "cannot set clipboard");
    }
}

pub fn clear(display: &gdk::Display) {
    let _ = display
        .clipboard()
        .set_content(None::<&gdk::ContentProvider>);
}

/// Reads files from the clipboard, if it holds any.
pub async fn read_files(display: &gdk::Display) -> Option<ClipboardFiles> {
    let clipboard = display.clipboard();
    let formats = clipboard.formats();
    tracing::debug!(formats = %formats, "reading files from the clipboard");
    if formats.contain_mime_type(GNOME_FORMAT)
        && let Some(bytes) = read_bytes(&clipboard, GNOME_FORMAT).await
        && let Some(files) = parse_gnome(&bytes)
    {
        return Some(files);
    }
    if formats.contains_type(gdk::FileList::static_type()) {
        let value = clipboard
            .read_value_future(gdk::FileList::static_type(), glib::Priority::DEFAULT)
            .await
            .ok()?;
        let list = value.get::<gdk::FileList>().ok()?;
        let paths: Vec<PathBuf> = list.files().iter().filter_map(|f| f.path()).collect();
        let cut = formats.contain_mime_type(KDE_CUT_FORMAT)
            && read_bytes(&clipboard, KDE_CUT_FORMAT)
                .await
                .is_some_and(|b| b.trim_ascii() == b"1");
        let kind = if cut {
            TransferKind::Move
        } else {
            TransferKind::Copy
        };
        return (!paths.is_empty()).then_some(ClipboardFiles { paths, kind });
    }
    None
}

async fn read_bytes(clipboard: &gdk::Clipboard, mime: &str) -> Option<Vec<u8>> {
    /// Clipboard file lists are small; refuse anything absurd.
    const LIMIT: usize = 4 * 1024 * 1024;
    let (stream, _) = clipboard
        .read_future(&[mime], glib::Priority::DEFAULT)
        .await
        .ok()?;
    let mut data = Vec::new();
    loop {
        let chunk = stream
            .read_bytes_future(64 * 1024, glib::Priority::DEFAULT)
            .await
            .ok()?;
        if chunk.is_empty() {
            break;
        }
        data.extend_from_slice(&chunk);
        if data.len() > LIMIT {
            return None;
        }
    }
    Some(data)
}

/// Parses `copy|cut\nfile:///…\n…`. Non-local URIs are ignored.
pub fn parse_gnome(bytes: &[u8]) -> Option<ClipboardFiles> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let kind = match lines.next()? {
        "cut" => TransferKind::Move,
        "copy" => TransferKind::Copy,
        _ => return None,
    };
    let paths: Vec<PathBuf> = lines
        .filter_map(|uri| glib::filename_from_uri(uri).ok())
        .map(|(path, _host)| path)
        .collect();
    (!paths.is_empty()).then_some(ClipboardFiles { paths, kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gnome_copy_and_cut() {
        let copy = parse_gnome(b"copy\nfile:///home/u/a%20b.txt\nfile:///tmp/x\n").unwrap();
        assert_eq!(copy.kind, TransferKind::Copy);
        assert_eq!(
            copy.paths,
            [PathBuf::from("/home/u/a b.txt"), PathBuf::from("/tmp/x")]
        );
        let cut = parse_gnome(b"cut\r\nfile:///tmp/x").unwrap();
        assert_eq!(cut.kind, TransferKind::Move);
    }

    #[test]
    fn non_utf8_paths_round_trip_through_uris() {
        use std::os::unix::ffi::OsStrExt;
        let parsed = parse_gnome(b"copy\nfile:///tmp/caf%E9").unwrap();
        assert_eq!(parsed.paths[0].as_os_str().as_bytes(), b"/tmp/caf\xe9");
    }

    #[test]
    fn rejects_other_content() {
        assert_eq!(parse_gnome(b"hello\nfile:///x"), None);
        assert_eq!(parse_gnome(b"copy\nhttps://example.com/x"), None);
        assert_eq!(parse_gnome(b""), None);
    }
}
