//! The spatial workspace: the active branch's panels in a horizontally
//! scrolling strip, and a branch bar to see and switch branches.
//!
//! Rendering only reads the workspace service. Panels near the active one
//! get full [`PanelView`]s; farther ones are drawn as slim buttons, so large
//! workspaces stay cheap (spatial spec §14). Focus Mode hides the other
//! panels without destroying them, so nothing reloads (§6).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use seshat_application::workspace::WorkspaceService;
use seshat_domain::entities::Location;
use seshat_domain::workspace::{BranchId, PanelId, Workspace};

use crate::panel_view::{HeaderInfo, PanelEvent, PanelView};
use crate::pathbar::address_text;

/// Panels within this distance of the active one get a full view.
const RENDER_RADIUS: usize = 3;
const INACTIVE_WIDTH: i32 = 300;
const MIN_ACTIVE_WIDTH: i32 = 460;
const COLLAPSED_WIDTH: i32 = 110;
const SCROLL_ANIMATION: Duration = Duration::from_millis(180);

#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceEvent {
    Panel(PanelId, PanelEvent),
    ActivateBranch(BranchId),
    CloseBranch(BranchId),
    /// The window was resized; panel widths need recomputing.
    Resized,
}

/// Scroll offsets of panels whose views were just dropped, to keep.
pub type DroppedScrolls = Vec<(PanelId, i32)>;

pub struct WorkspaceView {
    pub root: gtk::Box,
    branch_bar: gtk::Box,
    scroller: gtk::ScrolledWindow,
    strip: gtk::Box,
    panels: RefCell<HashMap<PanelId, PanelView>>,
    collapsed: RefCell<HashMap<PanelId, gtk::Button>>,
    branches_shown: RefCell<Vec<(BranchId, String, bool)>>,
    last_layout: Cell<Option<(PanelId, bool, i32)>>,
    on_event: Rc<dyn Fn(WorkspaceEvent)>,
}

impl std::fmt::Debug for WorkspaceView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceView")
            .field("panels", &self.panels.borrow().len())
            .finish_non_exhaustive()
    }
}

impl WorkspaceView {
    pub fn new(on_event: impl Fn(WorkspaceEvent) + 'static) -> Self {
        let on_event: Rc<dyn Fn(WorkspaceEvent)> = Rc::new(on_event);
        let branch_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .css_classes(["seshat-branch-bar"])
            .build();
        branch_bar.update_property(&[gtk::accessible::Property::Label("Branches")]);
        let branch_scroller = gtk::ScrolledWindow::builder()
            .child(&branch_bar)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hscrollbar_policy(gtk::PolicyType::External)
            .build();

        let strip = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["seshat-strip"])
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .child(&strip)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .build();
        scroller.hadjustment().connect_page_size_notify({
            let on_event = on_event.clone();
            move |_| on_event(WorkspaceEvent::Resized)
        });

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["seshat-workspace"])
            .build();
        root.append(&branch_scroller);
        root.append(&scroller);

        Self {
            root,
            branch_bar,
            scroller,
            strip,
            panels: RefCell::default(),
            collapsed: RefCell::default(),
            branches_shown: RefCell::default(),
            last_layout: Cell::new(None),
            on_event,
        }
    }

    /// Renders the workspace. Returns the panels that have full views (so
    /// their listings stay loaded) and the scroll offsets of panels whose
    /// views were dropped.
    pub fn render(&self, service: &WorkspaceService) -> (Vec<PanelId>, DroppedScrolls) {
        let ws = service.workspace();
        let path = ws.active_path();
        let active = ws.active_panel();
        let focus = ws.focus_mode();
        let index = path.iter().position(|&p| p == active).unwrap_or(0);
        let branch_number = ws
            .branches()
            .iter()
            .position(|b| b.id() == ws.active_branch().id())
            .map_or(1, |i| i + 1);

        let full: Vec<PanelId> = path
            .iter()
            .enumerate()
            .filter(|(i, _)| i.abs_diff(index) <= RENDER_RADIUS)
            .map(|(_, &p)| p)
            .collect();
        let on_path: HashSet<PanelId> = path.iter().copied().collect();

        // Drop views that are no longer on the active branch, or too far.
        let mut dropped = Vec::new();
        self.panels.borrow_mut().retain(|id, view| {
            let keep = full.contains(id);
            if !keep {
                dropped.push((*id, view.scroll_offset()));
                self.strip.remove(&view.root);
            }
            keep
        });
        self.collapsed.borrow_mut().retain(|id, button| {
            let keep = on_path.contains(id) && !full.contains(id);
            if !keep {
                self.strip.remove(button);
            }
            keep
        });

        let viewport = self.viewport_width();
        let active_width = if focus {
            viewport
        } else {
            (viewport / 2).max(MIN_ACTIVE_WIDTH).min(viewport.max(1))
        };

        let mut previous: Option<gtk::Widget> = None;
        for (i, &panel) in path.iter().enumerate() {
            let widget: gtk::Widget = if full.contains(&panel) {
                self.ensure_panel(panel);
                let panels = self.panels.borrow();
                let view = &panels[&panel];
                let p = ws.panel(panel).expect("path panel exists");
                let is_active = panel == active;
                let header = header_info(
                    ws,
                    panel,
                    is_active.then(|| branch_number.to_string()),
                    focus.then(|| (i.checked_sub(1).map(|j| path[j]), path.get(i + 1).copied())),
                );
                view.render(
                    &p.state,
                    service.listing(panel),
                    p.unavailable(),
                    &header,
                    is_active,
                );
                view.root.set_visible(!focus || is_active);
                view.root.set_hexpand(is_active);
                view.root.set_size_request(
                    if is_active {
                        active_width
                    } else {
                        INACTIVE_WIDTH
                    },
                    -1,
                );
                view.root.clone().upcast()
            } else {
                let button = self.ensure_collapsed(panel, ws);
                button.set_visible(!focus);
                button.upcast()
            };
            if widget.parent().is_none() {
                self.strip.append(&widget);
            }
            self.strip.reorder_child_after(&widget, previous.as_ref());
            previous = Some(widget);
        }

        self.render_branch_bar(ws);

        let layout = (active, focus, viewport);
        if self.last_layout.replace(Some(layout)) != Some(layout) {
            self.reveal(active);
        }
        (full, dropped)
    }

    /// Gives keyboard focus to the active panel.
    pub fn focus_panel(&self, panel: PanelId, cursor: Option<&Location>) {
        if let Some(view) = self.panels.borrow().get(&panel) {
            view.focus(cursor);
        }
    }

    pub fn with_panel<R>(&self, panel: PanelId, f: impl FnOnce(&PanelView) -> R) -> Option<R> {
        self.panels.borrow().get(&panel).map(f)
    }

    /// The panel and row that keyboard focus `widget` belongs to.
    pub fn panel_at(&self, widget: &gtk::Widget) -> Option<(PanelId, Option<Location>)> {
        self.panels
            .borrow()
            .values()
            .find(|v| v.files.contains(widget))
            .map(|v| (v.id, v.files.location_at_focus(widget)))
    }

    /// Current scroll offsets of all rendered panels (for persistence).
    pub fn scroll_offsets(&self) -> Vec<(PanelId, i32)> {
        self.panels
            .borrow()
            .values()
            .map(|v| (v.id, v.scroll_offset()))
            .collect()
    }

    fn viewport_width(&self) -> i32 {
        let page = self.scroller.hadjustment().page_size() as i32;
        if page > 0 { page } else { 760 }
    }

    fn ensure_panel(&self, panel: PanelId) {
        if self.panels.borrow().contains_key(&panel) {
            return;
        }
        let on_event = self.on_event.clone();
        let view = PanelView::new(panel, move |event| {
            on_event(WorkspaceEvent::Panel(panel, event))
        });
        self.panels.borrow_mut().insert(panel, view);
    }

    fn ensure_collapsed(&self, panel: PanelId, ws: &Workspace) -> gtk::Button {
        let title = ws
            .panel(panel)
            .map(|p| folder_title(p.location()))
            .unwrap_or_default();
        let mut collapsed = self.collapsed.borrow_mut();
        let button = collapsed.entry(panel).or_insert_with(|| {
            let label = gtk::Label::builder()
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let button = gtk::Button::builder()
                .child(&label)
                .css_classes(["flat", "seshat-panel-collapsed"])
                .width_request(COLLAPSED_WIDTH)
                .build();
            let on_event = self.on_event.clone();
            button.connect_clicked(move |_| {
                on_event(WorkspaceEvent::Panel(panel, PanelEvent::Activate))
            });
            button
        });
        if let Some(label) = button.child().and_downcast::<gtk::Label>() {
            label.set_label(&title);
        }
        button.set_tooltip_text(Some(&title));
        button.clone()
    }

    fn render_branch_bar(&self, ws: &Workspace) {
        let active = ws.active_branch().id();
        let branches: Vec<(BranchId, String, bool)> = ws
            .branches()
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let name = ws
                    .panel(b.tip())
                    .map(|p| folder_title(p.location()))
                    .unwrap_or_default();
                (b.id(), format!("{} {name}", i + 1), b.id() == active)
            })
            .collect();
        if *self.branches_shown.borrow() == branches {
            return;
        }
        while let Some(child) = self.branch_bar.first_child() {
            self.branch_bar.remove(&child);
        }
        let closable = branches.len() > 1;
        for (id, label, is_active) in &branches {
            let chip = gtk::Box::builder()
                .css_classes(if *is_active {
                    vec!["seshat-branch", "seshat-branch-active"]
                } else {
                    vec!["seshat-branch"]
                })
                .build();
            let open = gtk::Button::builder()
                .label(label.as_str())
                .css_classes(["flat"])
                .tooltip_text(if *is_active {
                    "Active branch"
                } else {
                    "Switch to this branch ([ and ])"
                })
                .build();
            let id = *id;
            let on_event = self.on_event.clone();
            open.connect_clicked(move |_| on_event(WorkspaceEvent::ActivateBranch(id)));
            chip.append(&open);
            if closable {
                let close = gtk::Button::builder()
                    .icon_name("window-close-symbolic")
                    .css_classes(["flat", "seshat-branch-close"])
                    .tooltip_text("Close branch (files are not affected)")
                    .build();
                let on_event = self.on_event.clone();
                close.connect_clicked(move |_| on_event(WorkspaceEvent::CloseBranch(id)));
                chip.append(&close);
            }
            self.branch_bar.append(&chip);
        }
        *self.branches_shown.borrow_mut() = branches;
    }

    /// Smoothly scrolls the strip so the active panel is fully visible.
    fn reveal(&self, panel: PanelId) {
        let Some(widget) = self
            .panels
            .borrow()
            .get(&panel)
            .map(|v| v.root.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        let strip = self.strip.clone();
        let adjustment = self.scroller.hadjustment();
        let scroller = self.scroller.clone();
        // After layout, so the new widths and positions are known.
        gtk::glib::idle_add_local_once(move || {
            let Some(bounds) = widget.compute_bounds(&strip) else {
                return;
            };
            let (left, right) = (
                f64::from(bounds.x()),
                f64::from(bounds.x() + bounds.width()),
            );
            let (value, page) = (adjustment.value(), adjustment.page_size());
            let target = if left < value {
                left
            } else if right > value + page {
                (right - page).min(left)
            } else {
                return;
            };
            let start = value;
            let begun = Instant::now();
            scroller.add_tick_callback(move |_, _| {
                let t = (begun.elapsed().as_secs_f64() / SCROLL_ANIMATION.as_secs_f64()).min(1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                adjustment.set_value(start + (target - start) * eased);
                if t < 1.0 {
                    gtk::glib::ControlFlow::Continue
                } else {
                    gtk::glib::ControlFlow::Break
                }
            });
        });
    }
}

/// A short name for a panel's folder.
pub fn folder_title(location: &Location) -> String {
    match location {
        Location::Trash(rel) if rel.as_os_str().is_empty() => "Trash".to_owned(),
        Location::Trash(rel) | Location::Path(rel) => rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_owned()),
    }
}

fn header_info(
    ws: &Workspace,
    panel: PanelId,
    branch_label: Option<String>,
    neighbours: Option<(Option<PanelId>, Option<PanelId>)>,
) -> HeaderInfo {
    let p = ws.panel(panel).expect("panel exists");
    let named = |id: PanelId| ws.panel(id).map(|p| (id, folder_title(p.location())));
    let (previous, next) = neighbours.unwrap_or((None, None));
    HeaderInfo {
        title: folder_title(p.location()),
        path: address_text(p.location()),
        in_trash: p.location().is_trash(),
        branch_label,
        forks: p.children().iter().filter_map(|&c| named(c)).collect(),
        previous: previous.and_then(named),
        next: next.and_then(named),
    }
}
