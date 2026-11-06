//! Left navigation pane.

use gtk::prelude::*;
use seshat_domain::entities::{StandardLocation, StandardLocationKind};

pub fn icon_name(kind: StandardLocationKind) -> &'static str {
    match kind {
        StandardLocationKind::Home => "user-home-symbolic",
        StandardLocationKind::Desktop => "user-desktop-symbolic",
        StandardLocationKind::Documents => "folder-documents-symbolic",
        StandardLocationKind::Downloads => "folder-download-symbolic",
        StandardLocationKind::Pictures => "folder-pictures-symbolic",
        StandardLocationKind::Music => "folder-music-symbolic",
        StandardLocationKind::Videos => "folder-videos-symbolic",
        StandardLocationKind::Trash => "user-trash-symbolic",
    }
}

/// Builds the navigation list. Row indices match `locations` indices.
pub fn build(locations: &[StandardLocation]) -> gtk::ListBox {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    list.update_property(&[gtk::accessible::Property::Label("Places")]);

    for location in locations {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        content.append(&gtk::Image::from_icon_name(icon_name(location.kind)));
        content.append(
            &gtk::Label::builder()
                .label(&location.label)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build(),
        );
        let row = gtk::ListBoxRow::builder().child(&content).build();
        row.set_tooltip_text(Some(&location.label));
        list.append(&row);
    }
    list
}
