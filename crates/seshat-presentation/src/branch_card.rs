//! A compact card for a panel that is shown but not as a full file list:
//! panels of inactive branches, and panels far from the active one.
//!
//! A card is drawn from the panel's saved state only (decision L1), so
//! showing it never reads the folder. Clicking it activates the panel.

use gtk::prelude::*;
use seshat_domain::entities::Location;
use seshat_domain::workspace::{PanelState, Unavailability};

/// Width of a card column with no full panel in it.
pub const CARD_WIDTH: i32 = 180;
/// Height of an inactive branch's lane.
pub const LANE_HEIGHT: i32 = 64;

/// What a card shows. Built from state, so it can be compared to skip
/// redundant updates and tested without GTK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardContent {
    pub title: String,
    pub detail: String,
    pub badge: Option<String>,
    pub tooltip: String,
    pub accessible: String,
}

impl CardContent {
    pub fn new(
        title: &str,
        path: &str,
        state: &PanelState,
        unavailable: Option<Unavailability>,
        branch_number: Option<usize>,
    ) -> Self {
        let selected = state.selection.len();
        let cursor = state.cursor.as_ref().map(name_of);
        let detail = match (unavailable, selected, cursor) {
            (Some(_), _, _) => "Unavailable".to_owned(),
            (None, 0, Some(cursor)) => cursor,
            (None, 0, None) => String::new(),
            (None, 1, _) => "1 selected".to_owned(),
            (None, n, _) => format!("{n} selected"),
        };
        let badge = branch_number.map(|n| n.to_string());
        let mut accessible = match branch_number {
            Some(n) => format!("Branch {n}, {title}"),
            None => title.to_owned(),
        };
        if !detail.is_empty() {
            accessible.push_str(", ");
            accessible.push_str(&detail);
        }
        Self {
            title: title.to_owned(),
            detail,
            badge,
            tooltip: path.to_owned(),
            accessible,
        }
    }
}

fn name_of(location: &Location) -> String {
    match location {
        Location::Path(p) | Location::Trash(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

#[derive(Debug)]
pub struct BranchCard {
    pub root: gtk::Button,
    title: gtk::Label,
    detail: gtk::Label,
    badge: gtk::Label,
    shown: std::cell::RefCell<Option<CardContent>>,
}

impl BranchCard {
    pub fn new(on_click: impl Fn() + 'static) -> Self {
        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["seshat-card-title"])
            .build();
        let badge = gtk::Label::builder()
            .css_classes(["seshat-branch-badge"])
            .visible(false)
            .build();
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        top.append(&title);
        top.append(&badge);
        let detail = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["dim-label"])
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .valign(gtk::Align::Start)
            .build();
        content.append(&top);
        content.append(&detail);
        let root = gtk::Button::builder()
            .child(&content)
            .css_classes(["flat", "seshat-card"])
            .width_request(CARD_WIDTH)
            .build();
        root.connect_clicked(move |_| on_click());
        Self {
            root,
            title,
            detail,
            badge,
            shown: std::cell::RefCell::default(),
        }
    }

    pub fn render(&self, content: CardContent, on_active_path: bool) {
        if on_active_path {
            self.root.add_css_class("seshat-card-on-path");
        } else {
            self.root.remove_css_class("seshat-card-on-path");
        }
        if self.shown.borrow().as_ref() == Some(&content) {
            return;
        }
        self.title.set_label(&content.title);
        self.detail.set_label(&content.detail);
        self.detail.set_visible(!content.detail.is_empty());
        self.badge.set_visible(content.badge.is_some());
        if let Some(badge) = &content.badge {
            self.badge.set_label(badge);
        }
        self.root.set_tooltip_text(Some(&content.tooltip));
        self.root
            .update_property(&[gtk::accessible::Property::Label(&content.accessible)]);
        self.shown.replace(Some(content));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(s: &str) -> Location {
        Location::Path(s.into())
    }

    #[test]
    fn cards_show_the_cursor_or_the_selection() {
        let mut state = PanelState::default();
        let plain = CardContent::new("Downloads", "/h/Downloads", &state, None, Some(2));
        assert_eq!(plain.detail, "");
        assert_eq!(plain.badge.as_deref(), Some("2"));
        assert_eq!(plain.accessible, "Branch 2, Downloads");

        state.cursor = Some(loc("/h/Downloads/big.iso"));
        assert_eq!(
            CardContent::new("Downloads", "", &state, None, None).detail,
            "big.iso"
        );

        state
            .selection
            .replace([loc("/h/Downloads/a"), loc("/h/Downloads/b")]);
        let selected = CardContent::new("Downloads", "", &state, None, None);
        assert_eq!(selected.detail, "2 selected");
        assert_eq!(selected.accessible, "Downloads, 2 selected");

        let gone = CardContent::new("USB", "", &state, Some(Unavailability::DeviceRemoved), None);
        assert_eq!(gone.detail, "Unavailable");
    }
}
