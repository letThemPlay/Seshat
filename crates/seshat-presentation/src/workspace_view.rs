//! The spatial workspace: branches side by side, with connectors, and a
//! branch bar to see and switch branches.
//!
//! Columns are tree depth and rows are branches ([`workspace_layout`]). A
//! shared ancestor is one cell spanning its branches' rows. The active
//! branch's row gets the height; other branches are compact lanes of cards.
//! Panels on the active branch near the active panel are full
//! [`PanelView`]s; everything else is a [`BranchCard`] drawn from saved
//! state, so showing a branch never reads its folders (decision L1).
//!
//! Rendering only reads the workspace service. Focus Mode hides everything
//! but the active panel without destroying anything, so nothing reloads.
//!
//! [`workspace_layout`]: crate::workspace_layout

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use seshat_application::preferences::WorkspaceLayout;
use seshat_application::workspace::WorkspaceService;
use seshat_domain::entities::Location;
use seshat_domain::workspace::{BranchId, PanelId, Workspace};

use crate::branch_card::{BranchCard, CardContent, LANE_HEIGHT};
use crate::connectors::ConnectorLane;
use crate::panel_view::{HeaderInfo, PanelEvent, PanelView};
use crate::pathbar::address_text;
use crate::workspace_layout::{self, Layout, LayoutOptions};

/// Panels within this many columns of the active one get a full view.
const RENDER_RADIUS: usize = 3;
const INACTIVE_WIDTH: i32 = 300;
const MIN_ACTIVE_WIDTH: i32 = 460;
/// The active branch keeps at least this much height; lanes get the rest.
const MIN_ACTIVE_HEIGHT: i32 = 300;
/// At most this many branches are shown at once (decision L4).
const MAX_ROWS: usize = 6;
const ROW_SPACING: i32 = 6;
const SCROLL_ANIMATION: Duration = Duration::from_millis(180);

#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceEvent {
    Panel(PanelId, PanelEvent),
    ActivateBranch(BranchId),
    CloseBranch(BranchId),
    /// "+N branches": show the branch overview.
    ShowBranches,
    /// The window was resized; widths and lanes need recomputing.
    Resized,
}

/// Scroll offsets of panels whose views were just dropped, to keep.
pub type DroppedScrolls = Vec<(PanelId, i32)>;

/// What a render did, for the window to follow up on.
#[derive(Debug, Default)]
pub struct RenderOutcome {
    /// Panels with full views, whose listings must stay loaded.
    pub visible: Vec<PanelId>,
    pub dropped: DroppedScrolls,
    /// The active panel's view was (re)attached and lost keyboard focus.
    pub refocus: bool,
}

type Placement = (i32, i32, i32, i32);

pub struct WorkspaceView {
    pub root: gtk::Box,
    branch_bar: gtk::Box,
    scroller: gtk::ScrolledWindow,
    grid: gtk::Grid,
    panels: RefCell<HashMap<PanelId, PanelView>>,
    cards: RefCell<HashMap<PanelId, BranchCard>>,
    lanes: RefCell<Vec<ConnectorLane>>,
    stubs: (gtk::Button, gtk::Button),
    /// Where each widget is attached in the grid.
    placed: RefCell<HashMap<gtk::Widget, Placement>>,
    branches_shown: RefCell<Vec<(BranchId, String, bool)>>,
    last_reveal: Cell<Option<(PanelId, bool, i32, i32)>>,
    /// Identifies the latest reveal animation; older ones stop.
    reveal_generation: Rc<Cell<u64>>,
    first_row: Cell<Option<usize>>,
    last_logged: RefCell<Option<(usize, usize, usize, usize)>>,
    /// Branches side by side or a single strip (decision L2).
    layout: WorkspaceLayout,
    on_event: Rc<dyn Fn(WorkspaceEvent)>,
}

impl std::fmt::Debug for WorkspaceView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceView")
            .field("panels", &self.panels.borrow().len())
            .field("cards", &self.cards.borrow().len())
            .finish_non_exhaustive()
    }
}

impl WorkspaceView {
    pub fn new(layout: WorkspaceLayout, on_event: impl Fn(WorkspaceEvent) + 'static) -> Self {
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

        let grid = gtk::Grid::builder()
            .row_spacing(ROW_SPACING)
            .css_classes(["seshat-strip"])
            .build();
        // GTK's own scroll-to-focus is off: it scrolls whenever keyboard
        // focus moves (e.g. to a row of a neighbouring panel), which would
        // push the active panel out of view. `reveal` keeps the active panel
        // visible instead.
        let viewport = gtk::Viewport::builder()
            .child(&grid)
            .scroll_to_focus(false)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .child(&viewport)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .build();
        for adjustment in [scroller.hadjustment(), scroller.vadjustment()] {
            let on_event = on_event.clone();
            adjustment.connect_page_size_notify(move |_| on_event(WorkspaceEvent::Resized));
        }
        // The strip got wider or narrower (a layout change, not the user
        // scrolling): keep the active panel in view.
        scroller.hadjustment().connect_upper_notify({
            let on_event = on_event.clone();
            move |_| on_event(WorkspaceEvent::Resized)
        });

        let stub = || {
            let button = gtk::Button::builder()
                .css_classes(["flat", "seshat-branch-stub"])
                .tooltip_text("Show all branches (:branch)")
                .build();
            let on_event = on_event.clone();
            button.connect_clicked(move |_| on_event(WorkspaceEvent::ShowBranches));
            button
        };
        let stubs = (stub(), stub());

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
            grid,
            panels: RefCell::default(),
            cards: RefCell::default(),
            lanes: RefCell::default(),
            stubs,
            placed: RefCell::default(),
            branches_shown: RefCell::default(),
            last_reveal: Cell::new(None),
            reveal_generation: Rc::default(),
            first_row: Cell::new(None),
            last_logged: RefCell::new(None),
            layout,
            on_event,
        }
    }

    /// Renders the workspace from the service.
    pub fn render(&self, service: &WorkspaceService) -> RenderOutcome {
        let ws = service.workspace();
        let active = ws.active_panel();
        let focus = ws.focus_mode();
        let max_rows = if self.layout == WorkspaceLayout::Strip {
            1
        } else {
            rows_for_height(self.viewport_height())
        };
        let layout = workspace_layout::compute(
            ws,
            LayoutOptions {
                max_rows,
                radius: RENDER_RADIUS,
            },
            self.first_row.get(),
        );
        self.first_row.set(Some(layout.first_row));
        self.log_layout(&layout);

        let full: Vec<PanelId> = layout
            .cells
            .iter()
            .filter(|c| c.full)
            .map(|c| c.panel)
            .collect();
        let carded: Vec<PanelId> = layout
            .cells
            .iter()
            .filter(|c| !c.full)
            .map(|c| c.panel)
            .collect();

        // Drop views and cards that are no longer shown that way.
        let mut dropped = Vec::new();
        {
            let mut placed = self.placed.borrow_mut();
            self.panels.borrow_mut().retain(|id, view| {
                let keep = full.contains(id);
                if !keep {
                    dropped.push((*id, view.scroll_offset()));
                    let widget = view.root.clone().upcast::<gtk::Widget>();
                    self.grid.remove(&widget);
                    placed.remove(&widget);
                }
                keep
            });
            self.cards.borrow_mut().retain(|id, card| {
                let keep = carded.contains(id);
                if !keep {
                    let widget = card.root.clone().upcast::<gtk::Widget>();
                    self.grid.remove(&widget);
                    placed.remove(&widget);
                }
                keep
            });
        }

        let viewport = self.viewport_width();
        let active_width = if focus {
            viewport
        } else {
            (viewport / 2).max(MIN_ACTIVE_WIDTH).min(viewport.max(1))
        };
        let path = ws.active_path();
        let branch_number = layout.first_row + layout.active_row + 1;
        let top = i32::from(layout.hidden_above > 0);

        let mut desired: Vec<(gtk::Widget, Placement)> = Vec::new();
        let mut widgets: HashMap<PanelId, gtk::Widget> = HashMap::new();
        for cell in &layout.cells {
            let placement = (
                (cell.column * 2) as i32,
                top + cell.row as i32,
                1,
                cell.span as i32,
            );
            let p = ws.panel(cell.panel).expect("laid out panel exists");
            let widget: gtk::Widget = if cell.full {
                self.ensure_panel(cell.panel);
                let panels = self.panels.borrow();
                let view = &panels[&cell.panel];
                let index = path.iter().position(|&x| x == cell.panel).unwrap_or(0);
                let header = header_info(
                    ws,
                    cell.panel,
                    cell.active.then(|| branch_number.to_string()),
                    focus.then(|| {
                        (
                            index.checked_sub(1).map(|j| path[j]),
                            path.get(index + 1).copied(),
                        )
                    }),
                );
                view.render(
                    &p.state,
                    service.listing(cell.panel),
                    p.unavailable(),
                    &header,
                    cell.active,
                );
                view.root.set_hexpand(cell.active);
                view.root.set_size_request(
                    if cell.active {
                        active_width
                    } else {
                        INACTIVE_WIDTH
                    },
                    -1,
                );
                view.root.clone().upcast()
            } else {
                self.ensure_card(cell.panel);
                let cards = self.cards.borrow();
                let card = &cards[&cell.panel];
                card.render(
                    CardContent::new(
                        &folder_title(p.location()),
                        &address_text(p.location()),
                        &p.state,
                        p.unavailable(),
                        cell.branch_number,
                    ),
                    cell.on_active_path,
                );
                card.root
                    .set_size_request(-1, if cell.on_active_path { -1 } else { LANE_HEIGHT });
                card.root.clone().upcast()
            };
            // The active branch's row takes the spare height; lanes stay
            // compact (a spanning shared panel does not stretch them, as the
            // active row already expands).
            widget.set_vexpand(cell.on_active_path);
            widget.set_visible(!focus || cell.active);
            widgets.insert(cell.panel, widget.clone());
            desired.push((widget, placement));
        }

        // Connector lanes between columns.
        let gaps = layout.columns.saturating_sub(1);
        {
            let mut lanes = self.lanes.borrow_mut();
            while lanes.len() < gaps {
                lanes.push(ConnectorLane::new());
            }
            for lane in lanes.drain(gaps..) {
                let widget = lane.root.upcast::<gtk::Widget>();
                if self.placed.borrow_mut().remove(&widget).is_some() {
                    self.grid.remove(&widget);
                }
            }
            for (column, lane) in lanes.iter().enumerate() {
                let link = |e: &&workspace_layout::Edge| {
                    Some((
                        widgets.get(&e.parent)?.clone(),
                        widgets.get(&e.child)?.clone(),
                    ))
                };
                let here = layout.edges.iter().filter(|e| e.column == column);
                let active_links = here
                    .clone()
                    .filter(|e| e.on_active_path)
                    .filter_map(|e| link(&e))
                    .collect();
                let inactive_links = here
                    .filter(|e| !e.on_active_path)
                    .filter_map(|e| link(&e))
                    .collect();
                lane.set_links(inactive_links, active_links);
                lane.root.set_visible(!focus);
                desired.push((
                    lane.root.clone().upcast(),
                    ((column * 2 + 1) as i32, top, 1, layout.rows as i32),
                ));
            }
        }

        // "+N branches" above and below the shown rows.
        let width = (layout.columns * 2 - 1) as i32;
        for (stub, hidden, row, side) in [
            (&self.stubs.0, layout.hidden_above, 0, "above"),
            (
                &self.stubs.1,
                layout.hidden_below,
                top + layout.rows as i32,
                "below",
            ),
        ] {
            if hidden > 0 && !focus {
                stub.set_label(&format!(
                    "+{hidden} branch{} {side}",
                    if hidden == 1 { "" } else { "es" }
                ));
                desired.push((stub.clone().upcast(), (0, row, width, 1)));
            }
        }

        let refocus = self.apply(desired, widgets.get(&active));
        self.render_branch_bar(ws);

        // Reveal again whenever the active panel, Focus Mode, the view width or
        // the strip's width changes; a manual scroll changes none of these.
        let strip_width = self.scroller.hadjustment().upper() as i32;
        let reveal = (active, focus, viewport, strip_width);
        if self.last_reveal.replace(Some(reveal)) != Some(reveal) {
            self.reveal(active);
        }
        RenderOutcome {
            visible: full,
            dropped,
            refocus,
        }
    }

    /// Attaches, moves and detaches grid children to match `desired`.
    /// Returns whether the active panel's widget was (re)attached.
    fn apply(&self, desired: Vec<(gtk::Widget, Placement)>, active: Option<&gtk::Widget>) -> bool {
        let mut placed = self.placed.borrow_mut();
        let keep: Vec<&gtk::Widget> = desired.iter().map(|(w, _)| w).collect();
        placed.retain(|widget, _| {
            let stays = keep.contains(&widget);
            if !stays {
                self.grid.remove(widget);
            }
            stays
        });
        let mut refocus = false;
        for (widget, placement) in desired {
            if placed.get(&widget) == Some(&placement) {
                continue;
            }
            if placed.remove(&widget).is_some() {
                self.grid.remove(&widget);
            }
            let (column, row, width, height) = placement;
            self.grid.attach(&widget, column, row, width, height);
            refocus |= Some(&widget) == active;
            placed.insert(widget, placement);
        }
        refocus
    }

    /// One debug line per layout change, for diagnostics and the UI tests.
    fn log_layout(&self, layout: &Layout) {
        let cards = layout.cells.iter().filter(|c| !c.full).count();
        let shape = (
            layout.rows,
            layout.columns,
            layout.hidden_above + layout.hidden_below,
            cards,
        );
        if self.last_logged.replace(Some(shape)) != Some(shape) {
            tracing::debug!(
                rows = shape.0,
                columns = shape.1,
                hidden = shape.2,
                cards = shape.3,
                connectors = layout.edges.len(),
                "workspace layout"
            );
        }
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

    fn viewport_height(&self) -> i32 {
        let height = self.scroller.height();
        if height > 0 { height } else { 560 }
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

    fn ensure_card(&self, panel: PanelId) {
        if self.cards.borrow().contains_key(&panel) {
            return;
        }
        let on_event = self.on_event.clone();
        let card =
            BranchCard::new(move || on_event(WorkspaceEvent::Panel(panel, PanelEvent::Activate)));
        self.cards.borrow_mut().insert(panel, card);
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

    /// Smoothly scrolls sideways until the active panel is fully visible.
    ///
    /// The remaining distance is measured again on every frame from where
    /// the panel actually is, so a panel that widens while it becomes
    /// active, or a layout that settles a frame late, is still revealed
    /// completely. A newer reveal takes over from an older one.
    fn reveal(&self, panel: PanelId) {
        let Some(widget) = self
            .panels
            .borrow()
            .get(&panel)
            .map(|v| v.root.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        let headers: Vec<gtk::Box> = self
            .panels
            .borrow()
            .values()
            .map(|v| v.header().clone())
            .collect();
        let generation = self.reveal_generation.get() + 1;
        self.reveal_generation.set(generation);
        let current = self.reveal_generation.clone();
        let adjustment = self.scroller.hadjustment();
        let begun = Instant::now();
        self.scroller.add_tick_callback(move |scroller, _| {
            if current.get() != generation {
                return gtk::glib::ControlFlow::Break; // a newer reveal took over
            }
            let elapsed = begun.elapsed();
            let Some(shift) = reveal_shift(&widget, scroller) else {
                return if elapsed < REVEAL_TIMEOUT {
                    gtk::glib::ControlFlow::Continue // not laid out yet
                } else {
                    gtk::glib::ControlFlow::Break
                };
            };
            let before = adjustment.value();
            if shift.abs() > 0.5 && elapsed < REVEAL_TIMEOUT {
                // Part of the way each frame: an ease-out that follows the
                // panel if it moves or grows meanwhile.
                // Rounded away from zero so the last pixels still move.
                let step = shift * REVEAL_STEP;
                adjustment.set_value(
                    before
                        + if step > 0.0 {
                            step.ceil()
                        } else {
                            step.floor()
                        },
                );
                let stuck =
                    (adjustment.value() - before).abs() < 0.01 && elapsed > SCROLL_ANIMATION;
                if !stuck {
                    return gtk::glib::ControlFlow::Continue;
                }
            } else if shift != 0.0 {
                adjustment.set_value(before + shift);
            }
            log_geometry(&widget, scroller, &headers);
            gtk::glib::ControlFlow::Break
        });
    }
}

/// Fraction of the remaining distance scrolled per frame.
const REVEAL_STEP: f64 = 0.3;
/// A reveal gives up after this long (e.g. if the panel never gets laid out).
const REVEAL_TIMEOUT: Duration = Duration::from_millis(800);
/// Space kept between the active panel and the edge of the workspace.
const REVEAL_MARGIN: f64 = ROW_SPACING as f64;

/// How far to scroll (positive: to the right) so `panel` is fully inside
/// `scroller`, with a small margin. A panel wider than the view is aligned
/// to its left edge. `None` while it has no allocation.
fn reveal_shift(panel: &gtk::Widget, scroller: &gtk::ScrolledWindow) -> Option<f64> {
    let bounds = panel.compute_bounds(scroller)?;
    let width = f64::from(scroller.width());
    if width <= 0.0 || bounds.width() <= 0.0 {
        return None;
    }
    Some(shift_into_view(
        f64::from(bounds.x()),
        f64::from(bounds.x() + bounds.width()),
        width,
        REVEAL_MARGIN,
    ))
}

/// The scroll needed to bring `[left, right]` inside `[margin, width - margin]`.
fn shift_into_view(left: f64, right: f64, width: f64, margin: f64) -> f64 {
    if right - left > width - 2.0 * margin || left < margin {
        left - margin
    } else if right > width - margin {
        right - (width - margin)
    } else {
        0.0
    }
}

/// One debug line after the active panel was revealed: its edges within
/// the visible area and the height of every panel header. The UI tests
/// check that the panel is fully visible and the headers are all the same
/// height.
fn log_geometry(active: &gtk::Widget, scroller: &gtk::ScrolledWindow, headers: &[gtk::Box]) {
    let Some(bounds) = active.compute_bounds(scroller) else {
        return;
    };
    let heights: Vec<String> = headers
        .iter()
        .filter(|h| h.is_mapped())
        .map(|h| h.height().to_string())
        .collect();
    tracing::debug!(
        active_left = bounds.x().round() as i32,
        active_right = (bounds.x() + bounds.width()).round() as i32,
        viewport = scroller.width(),
        headers = heights.join(","),
        "workspace geometry"
    );
}

/// How many branches fit: the active one keeps [`MIN_ACTIVE_HEIGHT`], each
/// other branch needs a lane. Too short for any lane means a single strip
/// (decision L2).
pub fn rows_for_height(height: i32) -> usize {
    let spare = (height - MIN_ACTIVE_HEIGHT).max(0);
    let lanes = (spare / (LANE_HEIGHT + ROW_SPACING)) as usize;
    (1 + lanes).min(MAX_ROWS)
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

#[cfg(test)]
mod tests {
    use super::{rows_for_height, shift_into_view};

    #[test]
    fn lanes_depend_on_height_and_are_capped() {
        assert_eq!(rows_for_height(200), 1, "too short: the single strip");
        assert_eq!(rows_for_height(300 + 69), 1);
        assert_eq!(rows_for_height(300 + 70), 2);
        assert_eq!(rows_for_height(300 + 3 * 70), 4);
        assert_eq!(rows_for_height(5000), 6, "at most six branches (L4)");
    }

    #[test]
    fn panels_are_scrolled_fully_into_view_with_a_margin() {
        // Visible already: nothing to do.
        assert_eq!(shift_into_view(100.0, 600.0, 1059.0, 6.0), 0.0);
        // The rightmost panel 7 px past the right edge.
        assert_eq!(shift_into_view(537.0, 1066.0, 1059.0, 6.0), 13.0);
        // Partly off the left edge.
        assert_eq!(shift_into_view(-124.0, 176.0, 1059.0, 6.0), -130.0);
        // Wider than the view: its left edge wins.
        assert_eq!(shift_into_view(200.0, 1400.0, 1059.0, 6.0), 194.0);
    }
}
