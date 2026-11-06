//! Main content pane: the file list.

use std::time::SystemTime;

use gtk::glib;
use gtk::prelude::*;
use relm4::typed_view::list::RelmListItem;
use seshat_domain::entities::{EntryKind, FileEntry};

/// One row of the file list.
#[derive(Debug)]
pub struct FileRow(pub FileEntry);

#[derive(Debug)]
pub struct FileRowWidgets {
    icon: gtk::Image,
    name: gtk::Label,
    origin: gtk::Label,
    size: gtk::Label,
    time: gtk::Label,
}

impl RelmListItem for FileRow {
    type Root = gtk::Box;
    type Widgets = FileRowWidgets;

    fn setup(_list_item: &gtk::ListItem) -> (gtk::Box, FileRowWidgets) {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let icon = gtk::Image::new();
        let name = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .width_chars(16)
            .build();
        let origin = dim_label(0.0, 0);
        origin.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        origin.set_max_width_chars(32);
        let size = dim_label(1.0, 10);
        let time = dim_label(1.0, 16);

        root.append(&icon);
        root.append(&name);
        root.append(&origin);
        root.append(&size);
        root.append(&time);

        (
            root,
            FileRowWidgets {
                icon,
                name,
                origin,
                size,
                time,
            },
        )
    }

    fn bind(&mut self, widgets: &mut FileRowWidgets, _root: &mut gtk::Box) {
        let entry = &self.0;
        widgets.icon.set_icon_name(Some(icon_name(entry.kind)));
        widgets.name.set_label(&entry.display_name);
        widgets.size.set_label(
            &entry
                .size
                .map(|s| glib::format_size(s).to_string())
                .unwrap_or_default(),
        );

        // Trashed items show where they came from and when they were deleted.
        match &entry.trash_info {
            Some(info) => {
                let from = info
                    .original_path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                widgets.origin.set_label(&from);
                widgets
                    .origin
                    .set_tooltip_text(Some(&format!("Originally in {from}")));
                widgets.origin.set_visible(true);
                widgets.time.set_label(&format_time(info.deleted_at));
                widgets.time.set_tooltip_text(Some("Deleted"));
            }
            None => {
                widgets.origin.set_visible(false);
                widgets.time.set_label(&format_time(entry.modified));
                widgets.time.set_tooltip_text(Some("Modified"));
            }
        }
    }
}

fn dim_label(xalign: f32, width_chars: i32) -> gtk::Label {
    let label = gtk::Label::builder()
        .xalign(xalign)
        .width_chars(width_chars)
        .build();
    label.add_css_class("dim-label");
    label
}

fn icon_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "folder-symbolic",
        EntryKind::Symlink { .. } => "insert-link-symbolic",
        EntryKind::File | EntryKind::Other => "text-x-generic-symbolic",
    }
}

fn format_time(time: Option<SystemTime>) -> String {
    let Some(secs) = time
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
    else {
        return String::new();
    };
    glib::DateTime::from_unix_local(secs)
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}
