//! The main content pane: list and grid views over one shared model.
//!
//! Both views share a single `gio::ListStore` and `gtk::MultiSelection`, so
//! switching views keeps the selection. Rows are recycled by GTK, so large
//! directories only create widgets for what is visible.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::SystemTime;

use gtk::prelude::*;
use gtk::{gdk, gio, glib, graphene};
use seshat_application::view::ViewMode;
use seshat_domain::entities::{EntryKind, FileEntry, Location};

/// What the views report to their owner.
#[derive(Debug, Clone, PartialEq)]
pub enum FileViewEvent {
    Activate(u32),
    /// Secondary click on an item (`Some`) or on empty space (`None`).
    /// Coordinates are relative to [`FileView::root`].
    ContextMenu {
        position: Option<u32>,
        x: f64,
        y: f64,
    },
    SelectionChanged,
    /// Files dropped from a drag, into a folder row (`Some`) or the shown
    /// folder (`None`).
    Drop {
        paths: Vec<std::path::PathBuf>,
        into: Option<u32>,
        copy: bool,
    },
}

type EventHandler = Rc<dyn Fn(FileViewEvent)>;

#[derive(Debug, Clone)]
pub struct FileView {
    /// Stack with the "list" and "grid" pages.
    pub root: gtk::Stack,
    store: gio::ListStore,
    selection: gtk::MultiSelection,
    list_scroll: gtk::ScrolledWindow,
    grid_scroll: gtk::ScrolledWindow,
    list: gtk::ListView,
    grid: gtk::GridView,
}

impl FileView {
    pub fn new(on_event: impl Fn(FileViewEvent) + 'static) -> Self {
        let on_event: EventHandler = Rc::new(on_event);
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::MultiSelection::new(Some(store.clone()));
        selection.connect_selection_changed({
            let on_event = on_event.clone();
            move |_, _, _| on_event(FileViewEvent::SelectionChanged)
        });

        let root = gtk::Stack::new();

        let list = gtk::ListView::new(
            Some(selection.clone()),
            Some(item_factory(&root, &selection, &on_event, Layout::Row)),
        );
        list.update_property(&[gtk::accessible::Property::Label("Files")]);
        let grid = gtk::GridView::new(
            Some(selection.clone()),
            Some(item_factory(&root, &selection, &on_event, Layout::Cell)),
        );
        grid.set_max_columns(64);
        grid.update_property(&[gtk::accessible::Property::Label("Files")]);
        for view in [list.upcast_ref::<gtk::Widget>(), grid.upcast_ref()] {
            view.set_can_focus(true);
        }
        list.connect_activate({
            let on_event = on_event.clone();
            move |_, position| on_event(FileViewEvent::Activate(position))
        });
        grid.connect_activate({
            let on_event = on_event.clone();
            move |_, position| on_event(FileViewEvent::Activate(position))
        });

        let list_scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build();
        let grid_scroll = gtk::ScrolledWindow::builder()
            .child(&grid)
            .vexpand(true)
            .build();
        for scroll in [&list_scroll, &grid_scroll] {
            background_menu_gesture(scroll, &root, &selection, &on_event);
            scroll.add_controller(drop_target(None, &on_event));
        }
        root.add_named(&list_scroll, Some("list"));
        root.add_named(&grid_scroll, Some("grid"));

        Self {
            root,
            store,
            selection,
            list_scroll,
            grid_scroll,
            list,
            grid,
        }
    }

    pub fn set_mode(&self, mode: ViewMode) {
        self.root.set_visible_child_name(mode.id());
    }

    pub fn len(&self) -> u32 {
        self.store.n_items()
    }

    pub fn entry(&self, position: u32) -> Option<FileEntry> {
        self.store.item(position).map(|obj| entry_of(&obj))
    }

    pub fn selected(&self) -> Vec<FileEntry> {
        (0..self.store.n_items())
            .filter(|&i| self.selection.is_selected(i))
            .filter_map(|i| self.entry(i))
            .collect()
    }

    /// Replaces the shown entries in one model update.
    ///
    /// With `preserve`, entries that were selected stay selected (matched by
    /// location) and the scroll position is kept, so live reloads don't
    /// disturb the user.
    pub fn replace(&self, entries: Vec<FileEntry>, preserve: bool) {
        let keep: HashSet<Location> = if preserve {
            self.selected().into_iter().map(|e| e.location).collect()
        } else {
            HashSet::new()
        };
        let scroll = preserve.then(|| {
            (
                self.list_scroll.vadjustment().value(),
                self.grid_scroll.vadjustment().value(),
            )
        });

        let reselect: Vec<u32> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| keep.contains(&e.location))
            .filter_map(|(i, _)| u32::try_from(i).ok())
            .collect();
        let objects: Vec<glib::Object> = entries
            .into_iter()
            .map(|e| glib::BoxedAnyObject::new(e).upcast())
            .collect();
        // Replacing the rows destroys the focused row; remember whether the
        // list had keyboard focus so it can be put back.
        let had_focus = self
            .root
            .root()
            .and_then(|r| gtk::prelude::RootExt::focus(&r))
            .is_some_and(|f| f.is_ancestor(&self.root));
        self.store.splice(0, self.store.n_items(), &objects);

        let selected = gtk::Bitset::new_empty();
        for i in reselect {
            selected.add(i);
        }
        self.selection
            .set_selection(&selected, &gtk::Bitset::new_range(0, self.store.n_items()));
        if had_focus {
            self.focus();
        }

        if let Some((list_y, grid_y)) = scroll {
            // Restore after GTK has re-laid out the new rows.
            let (list, grid) = (
                self.list_scroll.vadjustment(),
                self.grid_scroll.vadjustment(),
            );
            glib::idle_add_local_once(move || {
                list.set_value(list_y);
                grid.set_value(grid_y);
            });
        } else {
            self.list_scroll.vadjustment().set_value(0.0);
            self.grid_scroll.vadjustment().set_value(0.0);
        }
    }

    /// Moves keyboard focus into the results, selecting the first item if
    /// nothing is selected (e.g. after searching).
    pub fn focus_results(&self) {
        if self.store.n_items() == 0 {
            return; // nothing to focus; keep focus in the search box
        }
        if self.selected().is_empty() {
            self.selection.select_item(0, true);
        }
        // scroll_to(FOCUS) moves focus within the view but does not take it
        // from another widget (e.g. the search box), so grab it first.
        self.focus();
        let flags = gtk::ListScrollFlags::FOCUS;
        if self.root.visible_child_name().as_deref() == Some("grid") {
            self.grid.scroll_to(0, flags, None);
        } else {
            self.list.scroll_to(0, flags, None);
        }
    }

    pub fn focus(&self) {
        if let Some(scroll) = self
            .root
            .visible_child()
            .and_downcast::<gtk::ScrolledWindow>()
            && let Some(view) = scroll.child()
        {
            view.grab_focus();
        }
    }
}

fn entry_of(obj: &glib::Object) -> FileEntry {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .expect("file store holds BoxedAnyObject")
        .borrow::<FileEntry>()
        .clone()
}

#[derive(Clone, Copy)]
enum Layout {
    Row,
    Cell,
}

fn item_factory(
    root: &gtk::Stack,
    selection: &gtk::MultiSelection,
    on_event: &EventHandler,
    layout: Layout,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup({
        let root = root.downgrade();
        let selection = selection.clone();
        let on_event = on_event.clone();
        move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("ListItem");
            let widget = match layout {
                Layout::Row => build_row(),
                Layout::Cell => build_cell(),
            };
            add_item_context_menu(&widget, item, &root, &selection, &on_event);
            add_drag_source(&widget, item, &selection);
            widget.add_controller(drop_target(Some(item.downgrade()), &on_event));
            item.set_child(Some(&widget));
        }
    });

    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().expect("ListItem");
        let (Some(child), Some(obj)) = (item.child(), item.item()) else {
            return;
        };
        let entry = entry_of(&obj);
        match layout {
            Layout::Row => bind_row(&child, &entry),
            Layout::Cell => bind_cell(&child, &entry),
        }
    });
    factory
}

// ── Rows (list view) ────────────────────────────────────────────────────────

fn dim_label(xalign: f32, width_chars: i32) -> gtk::Label {
    let label = gtk::Label::builder()
        .xalign(xalign)
        .width_chars(width_chars)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    label.add_css_class("dim-label");
    label
}

/// Children, in order: icon, name, origin (Trash only), type, size, time.
fn build_row() -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&gtk::Image::new());
    row.append(
        &gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .width_chars(16)
            .build(),
    );
    let origin = dim_label(0.0, 0);
    origin.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    origin.set_max_width_chars(28);
    row.append(&origin);
    row.append(&dim_label(0.0, 14));
    row.append(&dim_label(1.0, 9));
    row.append(&dim_label(1.0, 16));
    row.upcast()
}

fn children(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut child = widget.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        out.push(c);
    }
    out
}

fn bind_row(widget: &gtk::Widget, entry: &FileEntry) {
    let parts = children(widget);
    let [icon, name, origin, kind, size, time] = parts.as_slice() else {
        return;
    };
    let label = |w: &gtk::Widget| w.downcast_ref::<gtk::Label>().cloned();
    let (Some(name), Some(origin), Some(kind), Some(size), Some(time)) = (
        label(name),
        label(origin),
        label(kind),
        label(size),
        label(time),
    ) else {
        return;
    };

    if let Some(icon) = icon.downcast_ref::<gtk::Image>() {
        icon.set_from_gicon(&entry_icon(entry, true));
    }
    name.set_label(&entry.display_name);
    kind.set_label(&type_description(entry));
    size.set_label(
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
            origin.set_tooltip_text(Some(&format!("Originally in {from}")));
            origin.set_label(&from);
            origin.set_visible(true);
            time.set_label(&format_time(info.deleted_at));
            time.set_tooltip_text(Some("Deleted"));
        }
        None => {
            origin.set_visible(false);
            time.set_label(&format_time(entry.modified));
            time.set_tooltip_text(Some("Modified"));
        }
    }
}

// ── Cells (grid view) ───────────────────────────────────────────────────────

/// Children, in order: icon, name.
fn build_cell() -> gtk::Widget {
    let cell = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .width_request(112)
        .css_classes(["seshat-grid-cell"])
        .build();
    cell.append(&gtk::Image::builder().pixel_size(48).build());
    cell.append(
        &gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .lines(2)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(14)
            .justify(gtk::Justification::Center)
            .build(),
    );
    cell.upcast()
}

fn bind_cell(widget: &gtk::Widget, entry: &FileEntry) {
    let parts = children(widget);
    let [icon, name] = parts.as_slice() else {
        return;
    };
    if let Some(icon) = icon.downcast_ref::<gtk::Image>() {
        icon.set_from_gicon(&entry_icon(entry, false));
    }
    if let Some(name) = name.downcast_ref::<gtk::Label>() {
        name.set_label(&entry.display_name);
    }
    widget.set_tooltip_text(Some(&entry.display_name));
}

// ── Interaction ─────────────────────────────────────────────────────────────

fn point_in(widget: &gtk::Widget, target: &impl IsA<gtk::Widget>, x: f64, y: f64) -> (f64, f64) {
    widget
        .compute_point(target, &graphene::Point::new(x as f32, y as f32))
        .map(|p| (f64::from(p.x()), f64::from(p.y())))
        .unwrap_or((x, y))
}

fn add_item_context_menu(
    widget: &gtk::Widget,
    item: &gtk::ListItem,
    root: &glib::WeakRef<gtk::Stack>,
    selection: &gtk::MultiSelection,
    on_event: &EventHandler,
) {
    let click = gtk::GestureClick::builder()
        .button(gdk::BUTTON_SECONDARY)
        .build();
    let item = item.downgrade();
    let root = root.clone();
    let selection = selection.clone();
    let on_event = on_event.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        let (Some(item), Some(root), Some(widget)) =
            (item.upgrade(), root.upgrade(), gesture.widget())
        else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        let position = item.position();
        // Right-clicking an unselected item acts on that item alone.
        if !selection.is_selected(position) {
            selection.select_item(position, true);
        }
        let (x, y) = point_in(&widget, &root, x, y);
        on_event(FileViewEvent::ContextMenu {
            position: Some(position),
            x,
            y,
        });
    });
    widget.add_controller(click);
}

fn background_menu_gesture(
    scroll: &gtk::ScrolledWindow,
    root: &gtk::Stack,
    selection: &gtk::MultiSelection,
    on_event: &EventHandler,
) {
    let click = gtk::GestureClick::builder()
        .button(gdk::BUTTON_SECONDARY)
        .build();
    let root = root.downgrade();
    let selection = selection.clone();
    let on_event = on_event.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        let (Some(root), Some(widget)) = (root.upgrade(), gesture.widget()) else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        selection.unselect_all();
        let (x, y) = point_in(&widget, &root, x, y);
        on_event(FileViewEvent::ContextMenu {
            position: None,
            x,
            y,
        });
    });
    scroll.add_controller(click);
}

/// Dragging items out offers the selected local files to other
/// applications (as `text/uri-list` via `GdkFileList`).
fn add_drag_source(widget: &gtk::Widget, item: &gtk::ListItem, selection: &gtk::MultiSelection) {
    let source = gtk::DragSource::builder()
        .actions(gdk::DragAction::COPY)
        .build();
    let item = item.downgrade();
    let selection = selection.clone();
    source.connect_prepare(move |_, _, _| {
        let item = item.upgrade()?;
        let position = item.position();
        if !selection.is_selected(position) {
            selection.select_item(position, true);
        }
        let model = selection.model()?;
        let files: Vec<gio::File> = (0..model.n_items())
            .filter(|&i| selection.is_selected(i))
            .filter_map(|i| model.item(i))
            .filter_map(|obj| entry_of(&obj).local_path().map(gio::File::for_path))
            .collect();
        if files.is_empty() {
            return None; // e.g. Trash items have no local path to offer
        }
        Some(gdk::ContentProvider::for_value(
            &gdk::FileList::from_array(&files).to_value(),
        ))
    });
    source.connect_drag_begin(|source, _| {
        if let Some(widget) = source.widget() {
            source.set_icon(Some(&gtk::WidgetPaintable::new(Some(&widget))), 0, 0);
        }
    });
    widget.add_controller(source);
}

// ── Formatting ──────────────────────────────────────────────────────────────

fn entry_icon(entry: &FileEntry, symbolic: bool) -> gio::Icon {
    let themed = |name: &str| -> gio::Icon {
        let name = if symbolic {
            format!("{name}-symbolic")
        } else {
            name.to_owned()
        };
        gio::ThemedIcon::new(&name).upcast()
    };
    match (entry.kind, entry.content_type.as_deref()) {
        (EntryKind::Directory | EntryKind::Symlink { to_directory: true }, _) => themed("folder"),
        (_, Some(content_type)) if symbolic => gio::content_type_get_symbolic_icon(content_type),
        (_, Some(content_type)) => gio::content_type_get_icon(content_type),
        _ => themed("text-x-generic"),
    }
}

fn type_description(entry: &FileEntry) -> String {
    match (entry.kind, entry.content_type.as_deref()) {
        (EntryKind::Symlink { to_directory: true }, _) => "Link to folder".to_owned(),
        (EntryKind::Symlink { .. }, _) => "Link".to_owned(),
        (EntryKind::Directory, _) => "Folder".to_owned(),
        (_, Some(content_type)) => gio::content_type_get_description(content_type).to_string(),
        _ => String::new(),
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

// ── Dropping files in ───────────────────────────────────────────────────────

/// Ctrl copies, Shift moves; otherwise drags from inside Seshat move and
/// drags from other applications copy (they may expect to keep the files).
fn drop_action(target: &gtk::DropTarget) -> gdk::DragAction {
    let state = target.current_event_state();
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        return gdk::DragAction::COPY;
    }
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        return gdk::DragAction::MOVE;
    }
    let internal = target.current_drop().and_then(|d| d.drag()).is_some();
    if internal {
        gdk::DragAction::MOVE
    } else {
        gdk::DragAction::COPY
    }
}

/// Accepts file lists. Row targets drop into the row's folder; other rows
/// and the background drop into the shown folder.
fn drop_target(
    item: Option<glib::WeakRef<gtk::ListItem>>,
    on_event: &EventHandler,
) -> gtk::DropTarget {
    let target = gtk::DropTarget::new(
        gdk::FileList::static_type(),
        gdk::DragAction::COPY | gdk::DragAction::MOVE,
    );
    target.connect_enter(|t, _, _| drop_action(t));
    target.connect_motion(|t, _, _| drop_action(t));
    let on_event = on_event.clone();
    target.connect_drop(move |t, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else {
            return false;
        };
        let paths: Vec<std::path::PathBuf> = list.files().iter().filter_map(|f| f.path()).collect();
        if paths.is_empty() {
            return false;
        }
        let into = item
            .as_ref()
            .and_then(|weak| weak.upgrade())
            .and_then(|item| {
                let browsable = item.item().is_some_and(|obj| entry_of(&obj).is_browsable());
                browsable.then(|| item.position())
            });
        on_event(FileViewEvent::Drop {
            paths,
            into,
            copy: drop_action(t) == gdk::DragAction::COPY,
        });
        true
    });
    target
}

impl FileView {
    /// Selects the entry at `location` (if shown) and scrolls to it.
    pub fn select_location(&self, location: &Location) -> bool {
        let Some(position) = (0..self.store.n_items()).find(|&i| {
            self.store
                .item(i)
                .is_some_and(|obj| entry_of(&obj).location == *location)
        }) else {
            return false;
        };
        let flags = gtk::ListScrollFlags::SELECT | gtk::ListScrollFlags::FOCUS;
        if self.root.visible_child_name().as_deref() == Some("grid") {
            self.grid.scroll_to(position, flags, None);
        } else {
            self.list.scroll_to(position, flags, None);
        }
        true
    }
}
