//! Connector lanes between columns of the branch layout: elbow lines from a
//! parent panel to its children.
//!
//! Colours come from CSS (`seshat-connector` uses the border token and
//! `seshat-connector-active` the accent token), so themes style them like
//! everything else. Inactive and active lines are two stacked drawing
//! areas, each drawing in its own CSS colour.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// Width of the gap between two columns.
pub const LANE_WIDTH: i32 = 24;

/// Lines meet panels at their header, not their middle, so a child in the
/// same row as its parent gets a straight line.
const HEADER_ANCHOR: f64 = 18.0;

/// Where a connector meets a cell whose top is `top` and height `height`.
pub fn anchor_y(top: f64, height: f64) -> f64 {
    top + (height / 2.0).min(HEADER_ANCHOR)
}

/// The elbow from a parent (left edge, `parent_y`) to a child (right edge,
/// `child_y`) across a lane `width` wide: across, down or up, across.
pub fn elbow(width: f64, parent_y: f64, child_y: f64) -> [(f64, f64); 4] {
    let middle = (width / 2.0).round();
    [
        (0.0, parent_y),
        (middle, parent_y),
        (middle, child_y),
        (width, child_y),
    ]
}

/// The connectors to draw, as (parent, child) widget pairs, so the
/// geometry is read from the real allocation at draw time.
pub type Links = Rc<RefCell<Vec<(gtk::Widget, gtk::Widget)>>>;

#[derive(Debug, Clone)]
pub struct ConnectorLane {
    pub root: gtk::Overlay,
    inactive_area: gtk::DrawingArea,
    active_area: gtk::DrawingArea,
    inactive: Links,
    active: Links,
}

impl ConnectorLane {
    pub fn new() -> Self {
        let inactive: Links = Rc::default();
        let active: Links = Rc::default();
        let inactive_area = area(&["seshat-connector"], inactive.clone(), 1.0);
        let active_area = area(
            &["seshat-connector", "seshat-connector-active"],
            active.clone(),
            2.0,
        );
        active_area.set_can_target(false);
        let root = gtk::Overlay::builder()
            .child(&inactive_area)
            .width_request(LANE_WIDTH)
            .can_focus(false)
            .can_target(false)
            .build();
        root.add_overlay(&active_area);
        Self {
            root,
            inactive_area,
            active_area,
            inactive,
            active,
        }
    }

    /// Replaces the lines and redraws once the new layout is allocated.
    pub fn set_links(
        &self,
        inactive: Vec<(gtk::Widget, gtk::Widget)>,
        active: Vec<(gtk::Widget, gtk::Widget)>,
    ) {
        *self.inactive.borrow_mut() = inactive;
        *self.active.borrow_mut() = active;
        self.redraw();
    }

    pub fn redraw(&self) {
        let (a, b) = (self.inactive_area.clone(), self.active_area.clone());
        gtk::glib::idle_add_local_once(move || {
            a.queue_draw();
            b.queue_draw();
        });
    }
}

impl Default for ConnectorLane {
    fn default() -> Self {
        Self::new()
    }
}

fn area(classes: &[&str], links: Links, line_width: f64) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .css_classes(classes.to_vec())
        .hexpand(false)
        .vexpand(true)
        .build();
    area.set_draw_func(move |area, cr, width, _height| {
        let color = area.color();
        cr.set_source_rgba(
            f64::from(color.red()),
            f64::from(color.green()),
            f64::from(color.blue()),
            f64::from(color.alpha()),
        );
        cr.set_line_width(line_width);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        for (parent, child) in links.borrow().iter() {
            let (Some(p), Some(c)) = (parent.compute_bounds(area), child.compute_bounds(area))
            else {
                continue;
            };
            if !parent.is_visible() || !child.is_visible() {
                continue;
            }
            let points = elbow(
                f64::from(width),
                anchor_y(f64::from(p.y()), f64::from(p.height())),
                anchor_y(f64::from(c.y()), f64::from(c.height())),
            );
            cr.move_to(points[0].0, points[0].1);
            for (x, y) in &points[1..] {
                cr.line_to(*x, *y);
            }
            let _ = cr.stroke();
        }
    });
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_row_children_get_a_straight_line() {
        let y = anchor_y(100.0, 400.0);
        assert_eq!(y, 118.0);
        let points = elbow(24.0, y, y);
        assert!(points.iter().all(|p| p.1 == y));
    }

    #[test]
    fn elbows_turn_in_the_middle_of_the_lane() {
        let points = elbow(24.0, 18.0, 82.0);
        assert_eq!(
            points,
            [(0.0, 18.0), (12.0, 18.0), (12.0, 82.0), (24.0, 82.0)]
        );
    }

    #[test]
    fn small_cells_are_met_in_their_middle() {
        assert_eq!(anchor_y(0.0, 20.0), 10.0);
    }
}
