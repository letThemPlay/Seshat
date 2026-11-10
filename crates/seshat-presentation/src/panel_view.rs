//! One workspace panel: a header and a file view for one folder.
//!
//! A `PanelView` holds no state of its own. It renders a panel's
//! [`PanelState`] and listing from the workspace service and reports user
//! input, so it can be destroyed and recreated at any time (spatial spec
//! §5.3) without losing the cursor, selection, scroll position or settings.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use seshat_application::ports::ListingError;
use seshat_application::workspace::{Content, PanelListing};
use seshat_domain::entities::Location;
use seshat_domain::workspace::{PanelId, PanelState, Unavailability};

use crate::files::{FileView, FileViewEvent};

/// What a panel reports to the workspace.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelEvent {
    Files(FileViewEvent),
    /// Clicked: make this the active panel.
    Activate,
    Retry,
    /// Go to the nearest ancestor panel that still exists.
    Parent,
    CloseBranch,
    /// Open one of this panel's child branches (fork menu).
    OpenChild(PanelId),
    /// Focus Mode hints: the previous or next panel of the branch.
    Neighbour(PanelId),
}

/// Everything the header needs besides the panel's own state.
#[derive(Debug, Clone, Default)]
pub struct HeaderInfo {
    pub title: String,
    pub path: String,
    pub in_trash: bool,
    /// "2", the number of the branch this panel belongs to when active.
    pub branch_label: Option<String>,
    /// Child branches starting here: panel and folder name.
    pub forks: Vec<(PanelId, String)>,
    /// In Focus Mode: previous and next panel on the branch.
    pub previous: Option<(PanelId, String)>,
    pub next: Option<(PanelId, String)>,
}

pub struct PanelView {
    pub id: PanelId,
    pub root: gtk::Box,
    pub files: FileView,
    header: gtk::Box,
    title: gtk::Label,
    branch: gtk::Label,
    forks: gtk::MenuButton,
    previous: gtk::Button,
    next: gtk::Button,
    stack: gtk::Stack,
    status_title: gtk::Label,
    actions: gtk::Box,
    /// Listing revision currently shown, to skip redundant model swaps.
    revision: Cell<u64>,
    shown_cursor: RefCell<Option<Location>>,
    shown_selection: RefCell<Option<Vec<Location>>>,
    shown_forks: RefCell<Vec<(PanelId, String)>>,
    /// Targets of the neighbour hint buttons.
    neighbours: Rc<RefCell<(Option<PanelId>, Option<PanelId>)>>,
    on_event: Rc<dyn Fn(PanelEvent)>,
}

impl std::fmt::Debug for PanelView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanelView")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl PanelView {
    pub fn new(id: PanelId, on_event: impl Fn(PanelEvent) + 'static) -> Self {
        let on_event: Rc<dyn Fn(PanelEvent)> = Rc::new(on_event);
        let files = FileView::new({
            let on_event = on_event.clone();
            move |event| on_event(PanelEvent::Files(event))
        });

        let previous = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .css_classes(["flat", "seshat-panel-hint"])
            .visible(false)
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .css_classes(["flat", "seshat-panel-hint"])
            .visible(false)
            .build();
        let neighbours: Rc<RefCell<(Option<PanelId>, Option<PanelId>)>> = Rc::default();
        for (button, pick) in [
            (
                &previous,
                (|n: &(Option<PanelId>, Option<PanelId>)| n.0) as fn(&_) -> _,
            ),
            (&next, |n| n.1),
        ] {
            let on_event = on_event.clone();
            let neighbours = neighbours.clone();
            button.connect_clicked(move |_| {
                if let Some(target) = pick(&neighbours.borrow()) {
                    on_event(PanelEvent::Neighbour(target));
                }
            });
        }

        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["seshat-panel-title"])
            .build();
        let branch = gtk::Label::builder()
            .css_classes(["seshat-branch-badge"])
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        let forks = gtk::MenuButton::builder()
            .icon_name("view-dual-symbolic")
            .css_classes(["flat", "seshat-fork"])
            .tooltip_text("Branches from here")
            .visible(false)
            .build();
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["seshat-panel-header"])
            .build();
        header.append(&previous);
        header.append(&title);
        header.append(&branch);
        header.append(&forks);
        header.append(&next);

        let status_title = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        let actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk::Align::Center)
            .visible(false)
            .build();
        for (label, event) in [
            ("Retry", PanelEvent::Retry),
            ("Go to Parent", PanelEvent::Parent),
            ("Close Branch", PanelEvent::CloseBranch),
        ] {
            let button = gtk::Button::with_label(label);
            let on_event = on_event.clone();
            button.connect_clicked(move |_| on_event(event.clone()));
            actions.append(&button);
        }
        let status_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .valign(gtk::Align::Center)
            .build();
        status_page.append(&status_title);
        status_page.append(&actions);

        let stack = gtk::Stack::builder().vexpand(true).build();
        stack.add_named(&files.root, Some("files"));
        stack.add_named(&status_page, Some("status"));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["seshat-panel"])
            .focusable(true)
            .build();
        root.append(&header);
        root.append(&stack);
        root.update_property(&[gtk::accessible::Property::Label("Folder panel")]);

        // Clicking anywhere in a panel activates it; the click still
        // reaches the file list.
        let click = gtk::GestureClick::builder()
            .button(0)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        click.connect_pressed({
            let on_event = on_event.clone();
            move |_, _, _, _| on_event(PanelEvent::Activate)
        });
        root.add_controller(click);

        Self {
            id,
            root,
            files,
            header: header.clone(),
            title,
            branch,
            forks,
            previous,
            next,
            stack,
            status_title,
            actions,
            revision: Cell::new(u64::MAX),
            shown_cursor: RefCell::new(None),
            shown_selection: RefCell::new(None),
            shown_forks: RefCell::default(),
            neighbours,
            on_event,
        }
    }

    /// Renders the panel from the model. Cheap when nothing changed: the
    /// file list is only replaced when the listing revision moved on.
    pub fn render(
        &self,
        state: &PanelState,
        listing: Option<&PanelListing>,
        unavailable: Option<Unavailability>,
        header: &HeaderInfo,
        active: bool,
    ) {
        self.render_header(header);
        set_class(&self.root, "seshat-panel-active", active);

        let content = listing.map_or(&Content::Loading, PanelListing::content);
        let (page, message) = match content {
            Content::Loading => ("status", "Loading…".to_owned()),
            Content::Ready => ("files", String::new()),
            Content::Empty if header.in_trash => ("status", "Trash is empty".to_owned()),
            Content::Empty => ("status", "This folder is empty".to_owned()),
            Content::NoResults => (
                "status",
                format!("No items match “{}”", state.options.query.trim()),
            ),
            Content::Failed(err) => ("status", error_message(err, unavailable)),
        };
        let failed = matches!(content, Content::Failed(_));
        set_class(&self.status_title, "seshat-error", failed);
        self.actions.set_visible(failed);
        self.status_title.set_label(&message);
        if self.stack.visible_child_name().as_deref() != Some(page) {
            self.stack.set_visible_child_name(page);
        }

        self.files.set_mode(state.mode);
        if let Some(listing) = listing
            && listing.revision() != self.revision.get()
        {
            let first = self.revision.get() == u64::MAX;
            self.revision.set(listing.revision());
            let started = std::time::Instant::now();
            self.files.replace(listing.shown().to_vec(), !first);
            tracing::debug!(
                rows = listing.shown().len(),
                elapsed_us = started.elapsed().as_micros() as u64,
                "panel view updated"
            );
            if first && let Some(scroll) = &state.scroll {
                self.files.restore_scroll_offset(f64::from(scroll.offset));
            }
            // Rows were rebuilt: draw the cursor and selection again.
            self.shown_cursor.replace(None);
            self.shown_selection.replace(None);
        }
        if self.shown_selection.borrow().as_deref() != Some(state.selection.items()) {
            let started = std::time::Instant::now();
            self.files.set_selected(state.selection.items());
            tracing::debug!(
                selected = state.selection.len(),
                elapsed_us = started.elapsed().as_micros() as u64,
                "selection applied"
            );
            self.shown_selection
                .replace(Some(state.selection.items().to_vec()));
        }
        if *self.shown_cursor.borrow() != state.cursor {
            self.shown_cursor.replace(state.cursor.clone());
            self.files.set_cursor(state.cursor.as_ref(), true, false);
        }
    }

    /// Gives the panel keyboard focus, on its cursor row if there is one.
    /// A panel showing a message (empty, loading, error) has no list to
    /// focus, so the panel itself takes focus; otherwise focus could stay
    /// on a hidden widget and window shortcuts would stop working.
    pub fn focus(&self, cursor: Option<&Location>) {
        if self.stack.visible_child_name().as_deref() != Some("files") {
            self.root.grab_focus();
            return;
        }
        self.files.focus();
        if cursor.is_some() {
            self.files.set_cursor(cursor, true, true);
        }
    }

    fn render_header(&self, header: &HeaderInfo) {
        self.title.set_label(&header.title);
        self.title.set_tooltip_text(Some(&header.path));
        match &header.branch_label {
            Some(label) => {
                self.branch.set_label(label);
                self.branch
                    .set_tooltip_text(Some(&format!("Branch {label}")));
                self.branch.set_visible(true);
            }
            None => self.branch.set_visible(false),
        }

        self.forks.set_visible(header.forks.len() > 1);
        if header.forks.len() > 1 && *self.shown_forks.borrow() != header.forks {
            self.shown_forks.replace(header.forks.clone());
            let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
            for (child, name) in &header.forks {
                let button = gtk::Button::builder()
                    .label(name.as_str())
                    .css_classes(["flat"])
                    .build();
                let on_event = self.on_event.clone();
                let child = *child;
                let forks = self.forks.clone();
                button.connect_clicked(move |_| {
                    forks.popdown();
                    on_event(PanelEvent::OpenChild(child));
                });
                list.append(&button);
            }
            let popover = gtk::Popover::builder().child(&list).build();
            self.forks.set_popover(Some(&popover));
            self.forks
                .set_tooltip_text(Some(&format!("{} branches from here", header.forks.len())));
        }

        *self.neighbours.borrow_mut() = (
            header.previous.as_ref().map(|p| p.0),
            header.next.as_ref().map(|n| n.0),
        );
        for (button, neighbour) in [
            (&self.previous, &header.previous),
            (&self.next, &header.next),
        ] {
            button.set_visible(neighbour.is_some());
            if let Some((_, name)) = neighbour {
                button.set_tooltip_text(Some(name));
            }
        }
    }

    /// The header bar, for layout diagnostics.
    pub fn header(&self) -> &gtk::Box {
        &self.header
    }

    /// The scroll offset to remember before this view goes away.
    pub fn scroll_offset(&self) -> i32 {
        self.files.scroll_offset() as i32
    }
}

fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

pub fn error_message(err: &ListingError, unavailable: Option<Unavailability>) -> String {
    match (err, unavailable) {
        (_, Some(Unavailability::DeviceRemoved)) => {
            "The device this folder is on was removed.".to_owned()
        }
        (ListingError::NotFound, _) | (_, Some(Unavailability::Missing)) => {
            "This folder no longer exists. It may have been deleted, moved or renamed.".to_owned()
        }
        (ListingError::PermissionDenied, _) => {
            "You don't have permission to view this folder.".to_owned()
        }
        (ListingError::NotADirectory, _) => "This is not a folder.".to_owned(),
        (ListingError::Unavailable(detail), _) => {
            format!("This location is unavailable.\n{detail}")
        }
        (ListingError::Other(detail), _) => format!("Could not open this folder.\n{detail}"),
    }
}
