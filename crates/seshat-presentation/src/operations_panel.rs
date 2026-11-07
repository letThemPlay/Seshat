//! Progress panel for file operations, driven purely by manager events.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use seshat_application::operations::{OperationEvent, OperationEventKind};
use seshat_domain::entities::Location;
use seshat_domain::operation::{Operation, OperationId, OperationKind, OperationStatus, Progress};

/// How long successful or cancelled operations stay visible.
const LINGER: Duration = Duration::from_secs(4);

#[derive(Debug, Clone)]
struct Row {
    root: gtk::Box,
    title: gtk::Label,
    detail: gtk::Label,
    bar: gtk::ProgressBar,
    button: gtk::Button,
}

#[derive(Clone)]
pub struct OperationsPanel {
    pub root: gtk::Revealer,
    list: gtk::Box,
    rows: Rc<std::cell::RefCell<HashMap<OperationId, Row>>>,
    on_cancel: Rc<dyn Fn(OperationId)>,
}

impl std::fmt::Debug for OperationsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationsPanel").finish_non_exhaustive()
    }
}

impl OperationsPanel {
    pub fn new(on_cancel: impl Fn(OperationId) + 'static) -> Self {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .css_classes(["seshat-operations"])
            .build();
        list.update_property(&[gtk::accessible::Property::Label("File operations")]);
        let root = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .child(&list)
            .build();
        Self {
            root,
            list,
            rows: Rc::default(),
            on_cancel: Rc::new(on_cancel),
        }
    }

    pub fn apply(&self, event: &OperationEvent) {
        let op = &event.operation;
        let row = self.row(op);
        let (title, detail) = describe(op);
        row.title.set_label(&title);
        row.detail.set_label(&detail);
        match progress_fraction(op) {
            Some(fraction) => row.bar.set_fraction(fraction),
            None if !op.is_finished() => row.bar.pulse(),
            None => row.bar.set_fraction(1.0),
        }
        row.root.remove_css_class("seshat-error");

        match event.kind {
            OperationEventKind::Completed | OperationEventKind::Cancelled
                if op.failures.is_empty() =>
            {
                row.button.set_visible(false);
                self.remove_later(op.id());
            }
            OperationEventKind::Completed
            | OperationEventKind::Cancelled
            | OperationEventKind::Failed => {
                // Keep problems visible until dismissed.
                row.root.add_css_class("seshat-error");
                row.button.set_icon_name("window-close-symbolic");
                row.button.set_tooltip_text(Some("Dismiss"));
                row.button.set_visible(true);
                row.detail.set_tooltip_text(Some(&failure_list(op)));
            }
            OperationEventKind::Cancelling => row.button.set_sensitive(false),
            _ => {}
        }
        self.root.set_reveal_child(!self.rows.borrow().is_empty());
    }

    fn row(&self, op: &Operation) -> Row {
        if let Some(row) = self.rows.borrow().get(&op.id()) {
            return row.clone();
        }
        let title = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        let detail = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["dim-label"])
            .build();
        let bar = gtk::ProgressBar::builder().pulse_step(0.1).build();
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();
        text.append(&title);
        text.append(&bar);
        text.append(&detail);
        let button = gtk::Button::builder()
            .icon_name("process-stop-symbolic")
            .tooltip_text("Cancel")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.append(&text);
        root.append(&button);

        let id = op.id();
        let rows = self.rows.clone();
        let list = self.list.clone();
        let revealer = self.root.clone();
        let on_cancel = self.on_cancel.clone();
        button.connect_clicked(move |_| {
            let finished = rows
                .borrow()
                .get(&id)
                .is_some_and(|r| r.root.has_css_class("seshat-error"));
            if finished {
                remove_row(&rows, &list, &revealer, id);
            } else {
                on_cancel(id);
            }
        });

        self.list.append(&root);
        let row = Row {
            root,
            title,
            detail,
            bar,
            button,
        };
        self.rows.borrow_mut().insert(id, row.clone());
        row
    }

    fn remove_later(&self, id: OperationId) {
        let rows = self.rows.clone();
        let list = self.list.clone();
        let revealer = self.root.clone();
        glib::timeout_add_local_once(LINGER, move || remove_row(&rows, &list, &revealer, id));
    }
}

fn remove_row(
    rows: &Rc<std::cell::RefCell<HashMap<OperationId, Row>>>,
    list: &gtk::Box,
    revealer: &gtk::Revealer,
    id: OperationId,
) {
    if let Some(row) = rows.borrow_mut().remove(&id) {
        list.remove(&row.root);
    }
    revealer.set_reveal_child(!rows.borrow().is_empty());
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn items(op: &Operation) -> String {
    match op.sources.as_slice() {
        [Location::Path(path)] => format!("“{}”", file_name(path)),
        [Location::Trash(rel)] => format!("“{}”", file_name(rel)),
        many => format!("{} items", many.len()),
    }
}

/// Title and detail line for an operation.
pub fn describe(op: &Operation) -> (String, String) {
    let target = op
        .destination
        .as_deref()
        .map(|d| format!(" to “{}”", file_name(d)))
        .unwrap_or_default();
    let title = match (op.kind(), op.status()) {
        (OperationKind::Rename, _) => format!(
            "Renaming {} to “{}”",
            items(op),
            op.destination.as_deref().map(file_name).unwrap_or_default()
        ),
        (OperationKind::CreateDirectory | OperationKind::CreateFile, _) => {
            format!("{} {}", op.kind().verb(), items(op))
        }
        (OperationKind::Trash, _) => format!("Moving {} to the Trash", items(op)),
        (OperationKind::Copy | OperationKind::Move, _) => {
            format!("{} {}{target}", op.kind().verb(), items(op))
        }
        _ => format!("{} {}", op.kind().verb(), items(op)),
    };

    let detail = match op.status() {
        OperationStatus::Queued => "Waiting…".to_owned(),
        OperationStatus::Started => "Preparing…".to_owned(),
        OperationStatus::Cancelling => "Cancelling…".to_owned(),
        OperationStatus::Cancelled => "Cancelled".to_owned(),
        OperationStatus::Failed => op.error.clone().unwrap_or_else(|| "Failed".to_owned()),
        OperationStatus::Completed if op.failures.is_empty() => "Done".to_owned(),
        OperationStatus::Completed => problems(op.failures.len()),
        OperationStatus::InProgress if op.awaiting_decision => {
            "Waiting for your decision…".to_owned()
        }
        OperationStatus::InProgress => counts(op),
    };
    (title, detail)
}

fn problems(n: usize) -> String {
    format!(
        "Done with {n} problem{} (hover for details)",
        if n == 1 { "" } else { "s" }
    )
}

fn counts(op: &Operation) -> String {
    let mut text = match op.files {
        Progress::Determinate { completed, total } => format!("{completed} of {total} items"),
        Progress::Indeterminate { completed } => format!("{completed} items"),
    };
    if let Progress::Determinate { completed, total } = op.bytes
        && total > 0
    {
        text.push_str(&format!(
            " — {} of {}",
            glib::format_size(completed),
            glib::format_size(total)
        ));
    }
    text
}

/// Bytes are the better measure when known; otherwise items.
fn progress_fraction(op: &Operation) -> Option<f64> {
    let pick = match op.bytes {
        Progress::Determinate { total, .. } if total > 0 => op.bytes,
        _ => op.files,
    };
    pick.percentage().map(|p| p / 100.0)
}

fn failure_list(op: &Operation) -> String {
    const SHOWN: usize = 8;
    let mut lines: Vec<String> = op
        .failures
        .iter()
        .take(SHOWN)
        .map(|f| format!("{}: {}", file_name(&f.path), f.message))
        .collect();
    if op.failures.len() > SHOWN {
        lines.push(format!("…and {} more", op.failures.len() - SHOWN));
    }
    if let Some(error) = &op.error {
        lines.insert(0, error.clone());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn op(kind: OperationKind, sources: &[&str], destination: Option<&str>) -> Operation {
        Operation::new(
            OperationId::new(),
            kind,
            sources.iter().map(|s| Location::Path(s.into())).collect(),
            destination.map(Into::into),
            SystemTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn titles() {
        let copy = op(OperationKind::Copy, &["/a/x.txt"], Some("/home/u/Docs"));
        assert_eq!(describe(&copy).0, "Copying “x.txt” to “Docs”");
        let trash = op(OperationKind::Trash, &["/a", "/b", "/c"], None);
        assert_eq!(describe(&trash).0, "Moving 3 items to the Trash");
        let rename = op(OperationKind::Rename, &["/d/old"], Some("/d/new"));
        assert_eq!(describe(&rename).0, "Renaming “old” to “new”");
    }

    #[test]
    fn details_follow_the_lifecycle() {
        let mut copy = op(OperationKind::Copy, &["/a"], Some("/b"));
        assert_eq!(describe(&copy).1, "Waiting…");
        copy.start(SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(describe(&copy).1, "Preparing…");
        copy.begin_work().unwrap();
        copy.files = Progress::Determinate {
            completed: 2,
            total: 5,
        };
        copy.bytes = Progress::Determinate {
            completed: 0,
            total: 0,
        };
        assert_eq!(describe(&copy).1, "2 of 5 items");
        assert_eq!(
            progress_fraction(&copy),
            Some(0.4),
            "files when there are no bytes"
        );
        copy.awaiting_decision = true;
        assert_eq!(describe(&copy).1, "Waiting for your decision…");
    }

    #[test]
    fn indeterminate_progress_has_no_fraction() {
        let mut trash = op(OperationKind::Trash, &["/a"], None);
        trash.files = Progress::indeterminate(3);
        assert_eq!(progress_fraction(&trash), None);
    }
}
