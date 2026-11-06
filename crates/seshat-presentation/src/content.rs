//! Main content pane: the file list.

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
    size: gtk::Label,
    modified: gtk::Label,
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
        let size = gtk::Label::builder().xalign(1.0).width_chars(10).build();
        size.add_css_class("dim-label");
        let modified = gtk::Label::builder().xalign(1.0).width_chars(16).build();
        modified.add_css_class("dim-label");

        root.append(&icon);
        root.append(&name);
        root.append(&size);
        root.append(&modified);

        (
            root,
            FileRowWidgets {
                icon,
                name,
                size,
                modified,
            },
        )
    }

    fn bind(&mut self, widgets: &mut FileRowWidgets, _root: &mut gtk::Box) {
        let entry = &self.0;
        widgets.icon.set_icon_name(Some(icon_name(entry.kind)));
        widgets.name.set_label(&entry.display_name());
        widgets.size.set_label(
            &entry
                .size
                .map(|s| glib::format_size(s).to_string())
                .unwrap_or_default(),
        );
        widgets.modified.set_label(&format_modified(entry));
    }
}

fn icon_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Directory => "folder-symbolic",
        EntryKind::Symlink => "insert-link-symbolic",
        EntryKind::File | EntryKind::Other => "text-x-generic-symbolic",
    }
}

fn format_modified(entry: &FileEntry) -> String {
    let Some(secs) = entry
        .modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
    else {
        return String::new();
    };
    glib::DateTime::from_unix_local(secs)
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}
