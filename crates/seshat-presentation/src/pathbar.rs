//! Breadcrumb bar with an editable address entry.

use gtk::prelude::*;
use seshat_application::navigation::Breadcrumb;
use seshat_domain::entities::Location;

#[derive(Debug, Clone)]
pub struct PathBar {
    pub root: gtk::Stack,
    crumbs: gtk::Box,
    pub entry: gtk::Entry,
}

impl PathBar {
    pub fn new() -> Self {
        let crumbs = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let crumbs_scroll = gtk::ScrolledWindow::builder()
            .child(&crumbs)
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .build();
        let entry = gtk::Entry::builder()
            .hexpand(true)
            .input_purpose(gtk::InputPurpose::Url)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label("Location")]);

        let root = gtk::Stack::builder()
            .hhomogeneous(false)
            .css_classes(["seshat-pathbar"])
            .build();
        root.add_named(&crumbs_scroll, Some("crumbs"));
        root.add_named(&entry, Some("entry"));
        Self {
            root,
            crumbs,
            entry,
        }
    }

    /// Replaces the breadcrumb buttons. The last crumb is the current folder.
    pub fn set_crumbs(&self, crumbs: &[Breadcrumb], on_click: impl Fn(Location) + Clone + 'static) {
        while let Some(child) = self.crumbs.first_child() {
            self.crumbs.remove(&child);
        }
        for (i, crumb) in crumbs.iter().enumerate() {
            if i > 0 {
                let separator = gtk::Label::new(Some("/"));
                separator.add_css_class("dim-label");
                self.crumbs.append(&separator);
            }
            let button = gtk::Button::builder()
                .label(&crumb.label)
                .css_classes(["flat"])
                .build();
            if i + 1 == crumbs.len() {
                button.add_css_class("current");
            }
            let location = crumb.location.clone();
            let on_click = on_click.clone();
            button.connect_clicked(move |_| on_click(location.clone()));
            self.crumbs.append(&button);
        }
    }

    pub fn show_entry(&self, text: &str) {
        self.entry.set_text(text);
        self.root.set_visible_child_name("entry");
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    pub fn show_crumbs(&self) {
        self.root.set_visible_child_name("crumbs");
    }
}

/// Text shown when editing the address of `location`.
pub fn address_text(location: &Location) -> String {
    match location {
        Location::Path(path) => path.to_string_lossy().into_owned(),
        Location::Trash(rel) => format!("trash:///{}", rel.to_string_lossy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_text_round_trips_through_the_parser() {
        let home = std::path::Path::new("/home/u");
        for location in [
            Location::Path("/home/u/Docs".into()),
            Location::trash_root(),
            Location::Trash("old/sub".into()),
        ] {
            let parsed =
                seshat_application::navigation::parse_address(&address_text(&location), home, None);
            assert_eq!(parsed, Ok(location));
        }
    }
}
