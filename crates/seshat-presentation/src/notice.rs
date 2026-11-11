//! Status-bar messages. This is the only place their text is formatted:
//! every message is a sentence, has a severity that decides how it looks
//! (and whether screen readers announce it), and counts are worded by one
//! helper.

use std::fmt::Display;
use std::sync::atomic::{AtomicU64, Ordering};

use seshat_application::interaction::Action;
use seshat_application::ports::ListingError;
use seshat_application::workspace::{PruneError, PruneOutcome, TransferKind};

/// How a message is shown (decision N1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Confirmations and information, in the normal status colour.
    Info,
    /// Refusals and failures, in the theme's error colour, and announced to
    /// screen readers (decision N2).
    Error,
}

/// Numbers notices as they are made, so the same message raised twice is
/// still two messages (and both are announced).
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A message for the status bar. Two notices are equal when they say the
/// same thing with the same severity, whatever their [`Notice::id`].
#[derive(Debug, Clone)]
pub struct Notice {
    id: u64,
    severity: Severity,
    text: String,
}

impl PartialEq for Notice {
    fn eq(&self, other: &Self) -> bool {
        self.severity == other.severity && self.text == other.text
    }
}

impl Eq for Notice {}

impl Notice {
    fn new(severity: Severity, text: String) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            severity,
            text,
        }
    }

    /// Information or a confirmation, as a sentence. Private: other modules
    /// call a named constructor below, so all wording stays in this file.
    fn info(text: impl Into<String>) -> Self {
        Self::new(Severity::Info, sentence(&text.into()))
    }

    /// Something was refused or failed, as a sentence. Private, like
    /// [`Notice::info`].
    fn error(text: impl Into<String>) -> Self {
        Self::new(Severity::Error, sentence(&text.into()))
    }

    /// A typed error from any layer, worded by its `Display`.
    pub fn from_error(err: &impl Display) -> Self {
        Self::error(err.to_string())
    }

    /// Text shown exactly as given, such as a list of completion matches,
    /// which is not a sentence.
    pub fn verbatim(text: impl Into<String>) -> Self {
        Self::new(Severity::Info, text.into())
    }

    /// Unique per notice made, even when the text repeats.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether screen readers should announce it when it appears
    /// (decision N2): errors only, so routine confirmations don't
    /// interrupt.
    pub fn announce(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }
}

/// `text` as a sentence: trimmed, with a capital first letter and a
/// closing full stop unless it already ends a sentence (decision N3).
fn sentence(text: &str) -> String {
    let text = text.trim();
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '!', '?', '…']) {
        out.push('.');
    }
    out
}

/// "1 item", "3 items": a number with the right form of a word.
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// After `db` or `dB`.
pub fn pruned(outcome: &PruneOutcome, undo_key: Option<&str>) -> Notice {
    let mut text = format!("Pruned {}", count(outcome.removed, "panel", "panels"));
    if outcome.ended_branch {
        text.push_str("; the branch ended");
    }
    match outcome.deselected {
        0 => {}
        1 => text.push_str("; 1 selected item was deselected"),
        n => text.push_str(&format!("; {n} selected items were deselected")),
    }
    if let Some(key) = undo_key {
        text.push_str(&format!(". Press {key} to undo"));
    }
    Notice::info(text)
}

/// After workspace undo (`u`).
pub fn restored(panels: usize) -> Notice {
    Notice::info(format!("Restored {}", count(panels, "panel", "panels")))
}

/// Items in the Trash an action had to leave out, if any.
pub fn left_out(n: usize) -> Option<Notice> {
    match n {
        0 => None,
        1 => Some(Notice::info("1 item in the Trash was left out")),
        n => Some(Notice::info(format!(
            "{n} items in the Trash were left out"
        ))),
    }
}

/// After copying paths to the clipboard.
pub fn copied_paths(n: usize) -> Notice {
    Notice::info(match n {
        1 => "Copied path to the clipboard".to_owned(),
        n => format!("Copied {n} paths to the clipboard"),
    })
}

/// A complete key binding for an action Visual Mode doesn't allow.
pub fn not_in_mode(action: Action) -> Notice {
    let what = match action {
        Action::PruneBack | Action::PruneBackSegment => "Pruning",
        _ => "This",
    };
    Notice::error(format!(
        "{what} isn't available in Visual Mode; press Esc first"
    ))
}

/// `db` or `dB` changed nothing.
pub fn not_pruned(err: &PruneError) -> Notice {
    match err {
        PruneError::FocusMode => Notice::from_error(err),
        PruneError::Refused(refusal) => Notice::error(format!("Nothing pruned: {refusal}")),
    }
}

/// `l` or `Enter` on a file in Visual Mode (decision C6).
pub fn files_not_in_visual() -> Notice {
    Notice::error("Files don't open in Visual Mode; press Esc first")
}

/// A branch number (`:branch N`, `:copy bN`) that doesn't exist.
pub fn no_branch(n: usize) -> Notice {
    Notice::error(format!("There is no branch {n}"))
}

/// `:copy` or `:move` with nothing selected.
pub fn select_first() -> Notice {
    Notice::error("Select files first (v to select, V for a range)")
}

/// A copy or move whose destination is the items' own folder.
pub fn already_in_folder() -> Notice {
    Notice::info("The items are already in that folder")
}

/// The active panel's folder disappeared.
pub fn folder_removed() -> Notice {
    Notice::error("The folder was deleted or moved")
}

/// Opening more files at once than `max`.
pub fn too_many_files(max: usize) -> Notice {
    Notice::error(format!("Select at most {max} files to open at once"))
}

/// A file could not be opened in its application.
pub fn open_failed(err: &impl Display) -> Notice {
    Notice::error(format!("Could not open: {err}"))
}

/// The first problem with the configured key bindings.
pub fn key_bindings(warning: &impl Display) -> Notice {
    Notice::error(format!("Key bindings: {warning} (see the log)"))
}

/// Paste with no files on the clipboard.
pub fn nothing_to_paste() -> Notice {
    Notice::error("The clipboard has no files to paste")
}

/// Paste into a panel that isn't a regular folder.
pub fn cannot_paste_here() -> Notice {
    Notice::error("Files can't be pasted here")
}

/// A drop onto something that isn't a regular folder.
pub fn cannot_drop_here() -> Notice {
    Notice::error("Files can't be dropped here")
}

/// Command-line completion found nothing.
pub fn no_matches() -> Notice {
    Notice::info("No matches")
}

/// Command-line completion could not read the folder.
pub fn completion_failed(err: &ListingError) -> Notice {
    Notice::error(crate::panel_view::error_message(err, None))
}

/// A file operation failed, with the manager's reason.
pub fn operation_failed(message: &str) -> Notice {
    Notice::error(message)
}

/// Ejecting or unmounting finished.
pub fn device_safe_to_remove() -> Notice {
    Notice::info("The device can be removed safely")
}

/// Mounting, unmounting or ejecting failed.
pub fn device_action_failed(err: &impl Display) -> Notice {
    Notice::error(format!("Device action failed: {err}"))
}

/// A device with open panels went away (decision D14).
pub fn device_removed() -> Notice {
    Notice::info("The device was removed. Its panels are kept until you close them")
}

/// The themes folder could not be created or opened.
pub fn themes_folder_failed(err: &impl Display) -> Notice {
    Notice::error(format!("Cannot open the themes folder: {err}"))
}

/// A theme file could not be loaded.
pub fn theme_not_loaded(id: &str, message: &str) -> Notice {
    Notice::error(format!("Theme “{id}” could not be loaded: {message}"))
}

/// The chosen custom theme is gone.
pub fn theme_unavailable(id: &str) -> Notice {
    Notice::error(format!(
        "Theme “{id}” is not available; following the system theme"
    ))
}

/// The chosen custom theme loaded with problems.
pub fn theme_has_issues(name: &str, issues: usize) -> Notice {
    Notice::error(format!(
        "Theme “{name}” has {}; see the log for details",
        count(issues, "issue", "issues")
    ))
}

/// Files waiting on the clipboard (decision D9), for the status line.
pub fn staged(count_staged: usize, kind: TransferKind) -> String {
    let verb = match kind {
        TransferKind::Copy => "copy",
        TransferKind::Move => "move",
    };
    format!(
        "{} ready to {verb}: paste (p or Ctrl+V) into a panel",
        count(count_staged, "item", "items")
    )
}

/// The status line's item count: "12 items", or "3 of 12 items" when a
/// search filters the panel.
pub fn item_count(shown: usize, total: usize) -> String {
    if shown == total {
        count(total, "item", "items")
    } else {
        format!("{shown} of {}", count(total, "item", "items"))
    }
}

#[cfg(test)]
mod tests {
    use seshat_domain::workspace::PanelId;

    use super::*;

    /// Worded like the application's `thiserror` errors: lowercase, no
    /// full stop.
    struct Lowercase(u32);

    impl std::fmt::Display for Lowercase {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "no such panel: {}", self.0)
        }
    }

    fn outcome(removed: usize, ended_branch: bool, deselected: usize) -> PruneOutcome {
        PruneOutcome {
            destination: PanelId::from_raw(1),
            removed,
            deselected,
            ended_branch,
        }
    }

    #[test]
    fn messages_become_sentences() {
        assert_eq!(Notice::info("no matches").text(), "No matches.");
        assert_eq!(Notice::info("  Done.  ").text(), "Done.");
        assert_eq!(Notice::info("Loading…").text(), "Loading…");
        assert_eq!(Notice::info("really?").text(), "Really?");
        assert_eq!(Notice::info("über").text(), "Über.");
        assert_eq!(Notice::info("").text(), "");
        assert_eq!(Notice::verbatim("a/  [b/]").text(), "a/  [b/]");
    }

    #[test]
    fn typed_errors_are_error_sentences() {
        let notice = Notice::from_error(&Lowercase(3));
        assert_eq!(notice.text(), "No such panel: 3.");
        assert_eq!(notice.severity(), Severity::Error);
        assert_eq!(Notice::info("x").severity(), Severity::Info);
    }

    #[test]
    fn counts_use_the_right_word() {
        assert_eq!(count(0, "item", "items"), "0 items");
        assert_eq!(count(1, "item", "items"), "1 item");
        assert_eq!(count(5, "item", "items"), "5 items");
        assert_eq!(item_count(12, 12), "12 items");
        assert_eq!(item_count(3, 12), "3 of 12 items");
        assert_eq!(item_count(1, 1), "1 item");
        assert_eq!(
            staged(1, TransferKind::Move),
            "1 item ready to move: paste (p or Ctrl+V) into a panel"
        );
    }

    #[test]
    fn prune_and_undo_messages() {
        assert_eq!(
            pruned(&outcome(1, false, 0), Some("u")).text(),
            "Pruned 1 panel. Press u to undo."
        );
        assert_eq!(
            pruned(&outcome(3, true, 2), None).text(),
            "Pruned 3 panels; the branch ended; 2 selected items were deselected."
        );
        assert_eq!(
            pruned(&outcome(2, false, 1), None).text(),
            "Pruned 2 panels; 1 selected item was deselected."
        );
        assert_eq!(restored(1).text(), "Restored 1 panel.");
        assert_eq!(restored(4).text(), "Restored 4 panels.");
    }

    #[test]
    fn small_confirmations() {
        assert_eq!(left_out(0), None);
        assert_eq!(
            left_out(1).unwrap().text(),
            "1 item in the Trash was left out."
        );
        assert_eq!(
            left_out(2).unwrap().text(),
            "2 items in the Trash were left out."
        );
        assert_eq!(copied_paths(1).text(), "Copied path to the clipboard.");
        assert_eq!(copied_paths(3).text(), "Copied 3 paths to the clipboard.");
    }

    #[test]
    fn only_errors_are_announced_and_each_notice_is_distinct() {
        let first = Notice::error("nothing pruned");
        let again = Notice::error("nothing pruned");
        assert_eq!(first, again, "equal by what they say");
        assert_ne!(first.id(), again.id(), "but two separate messages");
        assert!(first.announce());
        assert!(!Notice::info("copied").announce());
        assert!(!Notice::verbatim("a/  b/").announce());
    }

    /// The named constructors keep the wording and severity the call sites
    /// had before they moved here.
    #[test]
    fn named_messages_keep_their_wording_and_severity() {
        use Severity::{Error, Info};
        let cases = [
            (
                not_pruned(&PruneError::FocusMode),
                "Pruning isn't available in Focus Mode.",
                Error,
            ),
            (
                files_not_in_visual(),
                "Files don't open in Visual Mode; press Esc first.",
                Error,
            ),
            (no_branch(4), "There is no branch 4.", Error),
            (
                select_first(),
                "Select files first (v to select, V for a range).",
                Error,
            ),
            (
                already_in_folder(),
                "The items are already in that folder.",
                Info,
            ),
            (folder_removed(), "The folder was deleted or moved.", Error),
            (
                too_many_files(20),
                "Select at most 20 files to open at once.",
                Error,
            ),
            (open_failed(&"no app"), "Could not open: no app.", Error),
            (
                key_bindings(&"bad key"),
                "Key bindings: bad key (see the log).",
                Error,
            ),
            (
                nothing_to_paste(),
                "The clipboard has no files to paste.",
                Error,
            ),
            (cannot_paste_here(), "Files can't be pasted here.", Error),
            (cannot_drop_here(), "Files can't be dropped here.", Error),
            (no_matches(), "No matches.", Info),
            (operation_failed("disk full"), "Disk full.", Error),
            (
                device_safe_to_remove(),
                "The device can be removed safely.",
                Info,
            ),
            (
                device_action_failed(&"busy"),
                "Device action failed: busy.",
                Error,
            ),
            (
                device_removed(),
                "The device was removed. Its panels are kept until you close them.",
                Info,
            ),
            (
                themes_folder_failed(&"read-only"),
                "Cannot open the themes folder: read-only.",
                Error,
            ),
            (
                theme_not_loaded("dusk", "bad colour"),
                "Theme “dusk” could not be loaded: bad colour.",
                Error,
            ),
            (
                theme_unavailable("dusk"),
                "Theme “dusk” is not available; following the system theme.",
                Error,
            ),
            (
                theme_has_issues("Dusk", 1),
                "Theme “Dusk” has 1 issue; see the log for details.",
                Error,
            ),
        ];
        for (notice, text, severity) in cases {
            assert_eq!(notice.text(), text);
            assert_eq!(notice.severity(), severity, "{text}");
        }
    }

    #[test]
    fn the_mode_message_names_the_action() {
        let prune = not_in_mode(Action::PruneBackSegment);
        assert_eq!(
            prune.text(),
            "Pruning isn't available in Visual Mode; press Esc first."
        );
        assert_eq!(prune.severity(), Severity::Error);
        assert_eq!(
            not_in_mode(Action::Paste).text(),
            "This isn't available in Visual Mode; press Esc first."
        );
    }
}
