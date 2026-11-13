//! Where each panel goes in the side-by-side branch layout. Pure geometry
//! derived from the workspace, with no GTK, so it is unit-tested.
//!
//! Columns are tree depth and rows are branches, in the same depth-first
//! order `[` and `]` cycle through. In that order every panel's branches
//! occupy adjacent rows, so a shared ancestor is one cell spanning them
//! (spatial spec §3.3, §3.8). A panel's column depends only on its depth,
//! so switching branches never moves panels sideways (§4.4).

use seshat_domain::workspace::{BranchId, PanelId, Workspace};

/// One panel's place in the grid. Rows are relative to the visible window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub panel: PanelId,
    pub column: usize,
    pub row: usize,
    pub span: usize,
    /// On the active branch's path.
    pub on_active_path: bool,
    pub active: bool,
    /// Rendered as a full panel (otherwise as a compact card).
    pub full: bool,
    /// For a branch's tip: its number in `[`/`]` order (1-based).
    pub branch_number: Option<usize>,
}

/// A connector from a parent panel to a child panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub parent: PanelId,
    pub child: PanelId,
    /// The column of the parent; the connector sits after it.
    pub column: usize,
    pub on_active_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub cells: Vec<Cell>,
    pub edges: Vec<Edge>,
    pub columns: usize,
    /// Rows shown (at most the requested maximum).
    pub rows: usize,
    /// Index of the first shown branch among all branches.
    pub first_row: usize,
    /// The active branch's row, relative to the shown rows.
    pub active_row: usize,
    /// Branches not shown above and below (decision L4).
    pub hidden_above: usize,
    pub hidden_below: usize,
    /// Branches of the shown rows, in order.
    pub branches: Vec<BranchId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutOptions {
    /// Rows to show at most, the active branch's included. 1 is the
    /// single-strip layout.
    pub max_rows: usize,
    /// Panels on the active branch within this many columns of the active
    /// panel are full panels; everything else is a card.
    pub radius: usize,
}

/// Computes the layout. `previous_first_row` keeps the shown window where
/// it was while the active branch stays inside it, so switching between
/// visible branches does not scroll the rows.
pub fn compute(
    ws: &Workspace,
    options: LayoutOptions,
    previous_first_row: Option<usize>,
) -> Layout {
    let branches: Vec<BranchId> = ws.branches().iter().map(|b| b.id()).collect();
    let total = branches.len();
    let active_branch = ws.active_branch().id();
    let active_index = branches
        .iter()
        .position(|&b| b == active_branch)
        .unwrap_or(0);
    let shown = options.max_rows.clamp(1, total.max(1));
    let first_row = match previous_first_row {
        Some(p) if p + shown <= total && (p..p + shown).contains(&active_index) => p,
        _ => active_index
            .saturating_sub((shown - 1) / 2)
            .min(total - shown),
    };
    let window = first_row..first_row + shown;

    // Depth and leaf range of every panel, in the same depth-first order
    // as `Workspace::branches`.
    let mut placed: Vec<Placed> = Vec::new();
    let mut next_leaf = 0;
    for &root in ws.roots() {
        place(ws, root, 0, &mut next_leaf, &mut placed);
    }

    let path = ws.active_path();
    let active = ws.active_panel();
    let active_depth = path.iter().position(|&p| p == active).unwrap_or(0);
    let mut cells = Vec::new();
    for &Placed {
        panel,
        depth,
        ref leaves,
    } in &placed
    {
        let from = leaves.start.max(window.start);
        let to = leaves.end.min(window.end);
        if from >= to {
            continue;
        }
        let on_active_path = path.contains(&panel);
        let is_tip = leaves.len() == 1 && ws.panel(panel).is_some_and(|p| p.is_leaf());
        cells.push(Cell {
            panel,
            column: depth,
            row: from - window.start,
            span: to - from,
            on_active_path,
            active: panel == active,
            full: on_active_path && depth.abs_diff(active_depth) <= options.radius,
            branch_number: is_tip.then_some(leaves.start + 1),
        });
    }

    let shown_panels: Vec<PanelId> = cells.iter().map(|c| c.panel).collect();
    let mut edges = Vec::new();
    for cell in &cells {
        let Some(parent) = ws.panel(cell.panel).and_then(|p| p.parent()) else {
            continue;
        };
        if shown_panels.contains(&parent) {
            edges.push(Edge {
                parent,
                child: cell.panel,
                column: cell.column - 1,
                on_active_path: cell.on_active_path && path.contains(&parent),
            });
        }
    }

    Layout {
        columns: cells.iter().map(|c| c.column + 1).max().unwrap_or(1),
        rows: shown,
        first_row,
        active_row: active_index - first_row,
        hidden_above: first_row,
        hidden_below: total - window.end,
        branches: branches[window].to_vec(),
        cells,
        edges,
    }
}

/// A panel's column and the rows (leaves below it) it spans.
struct Placed {
    panel: PanelId,
    depth: usize,
    leaves: std::ops::Range<usize>,
}

fn place(
    ws: &Workspace,
    panel: PanelId,
    depth: usize,
    next_leaf: &mut usize,
    out: &mut Vec<Placed>,
) {
    let Some(p) = ws.panel(panel) else {
        return;
    };
    let index = out.len();
    out.push(Placed {
        panel,
        depth,
        leaves: *next_leaf..*next_leaf,
    });
    if p.children().is_empty() {
        *next_leaf += 1;
    }
    for &child in p.children() {
        place(ws, child, depth + 1, next_leaf, out);
    }
    out[index].leaves.end = *next_leaf;
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_domain::Direction;
    use seshat_domain::entities::Location;
    use seshat_domain::workspace::PanelState;

    fn loc(s: &str) -> Location {
        Location::Path(s.into())
    }

    const ALL: LayoutOptions = LayoutOptions {
        max_rows: 6,
        radius: 3,
    };

    /// Home → Documents → Projects; back to Home; Downloads.
    fn scenario() -> (Workspace, [PanelId; 4]) {
        let mut ws = Workspace::new(loc("/h"), PanelState::default());
        let home = ws.active_panel();
        let docs = ws
            .open_child(home, loc("/h/Documents"), None)
            .unwrap()
            .panel;
        let projects = ws
            .open_child(docs, loc("/h/Documents/Projects"), None)
            .unwrap()
            .panel;
        ws.move_left();
        ws.move_left();
        let downloads = ws
            .open_child(home, loc("/h/Downloads"), None)
            .unwrap()
            .panel;
        (ws, [home, docs, projects, downloads])
    }

    fn cell(layout: &Layout, panel: PanelId) -> Cell {
        *layout
            .cells
            .iter()
            .find(|c| c.panel == panel)
            .expect("cell shown")
    }

    #[test]
    fn shared_ancestor_spans_its_branches_and_columns_are_depth() {
        let (ws, [home, docs, projects, downloads]) = scenario();
        let layout = compute(&ws, ALL, None);
        assert_eq!((layout.rows, layout.columns), (2, 3));
        let h = cell(&layout, home);
        assert_eq!(
            (h.column, h.row, h.span),
            (0, 0, 2),
            "Home once, over both rows"
        );
        assert_eq!(
            (cell(&layout, docs).column, cell(&layout, docs).row),
            (1, 0)
        );
        assert_eq!(cell(&layout, projects).column, 2);
        let d = cell(&layout, downloads);
        assert_eq!((d.column, d.row, d.span), (1, 1, 1));
        assert_eq!(layout.cells.len(), 4, "no panel is drawn twice");
    }

    #[test]
    fn rows_follow_bracket_order_and_tips_are_numbered() {
        let (ws, [_, _, projects, downloads]) = scenario();
        let layout = compute(&ws, ALL, None);
        let order: Vec<BranchId> = ws.branches().iter().map(|b| b.id()).collect();
        assert_eq!(layout.branches, order);
        assert_eq!(cell(&layout, projects).branch_number, Some(1));
        assert_eq!(cell(&layout, downloads).branch_number, Some(2));
        assert_eq!(layout.active_row, 1, "Downloads is active");
    }

    #[test]
    fn switching_branch_moves_no_panel() {
        let (mut ws, _) = scenario();
        let before = compute(&ws, ALL, None);
        ws.cycle_branch(Direction::Next);
        let after = compute(&ws, ALL, Some(before.first_row));
        let place = |l: &Layout| -> Vec<(PanelId, usize, usize, usize)> {
            l.cells
                .iter()
                .map(|c| (c.panel, c.column, c.row, c.span))
                .collect()
        };
        assert_eq!(place(&before), place(&after));
        assert_ne!(before.active_row, after.active_row);
    }

    #[test]
    fn only_the_active_path_near_the_active_panel_is_full() {
        let (ws, [home, docs, projects, downloads]) = scenario();
        let layout = compute(&ws, ALL, None);
        assert!(cell(&layout, home).full && cell(&layout, home).on_active_path);
        assert!(cell(&layout, downloads).full && cell(&layout, downloads).active);
        assert!(
            !cell(&layout, docs).full,
            "inactive branches are cards (L1)"
        );
        assert!(!cell(&layout, projects).full);

        let narrow = compute(&ws, LayoutOptions { radius: 0, ..ALL }, None);
        assert!(!cell(&narrow, home).full, "beyond the radius: a card");
        assert!(cell(&narrow, downloads).full);
    }

    #[test]
    fn edges_connect_parents_to_children_and_mark_the_active_path() {
        let (ws, [home, docs, projects, downloads]) = scenario();
        let layout = compute(&ws, ALL, None);
        let edge = |p, c| {
            layout
                .edges
                .iter()
                .find(|e| e.parent == p && e.child == c)
                .copied()
        };
        assert_eq!(
            edge(home, docs).map(|e| (e.column, e.on_active_path)),
            Some((0, false))
        );
        assert_eq!(edge(docs, projects).map(|e| e.column), Some(1));
        assert_eq!(edge(home, downloads).map(|e| e.on_active_path), Some(true));
        assert_eq!(layout.edges.len(), 3);
    }

    #[test]
    fn many_branches_overflow_into_hidden_counts_around_the_active_one() {
        let mut ws = Workspace::new(loc("/h"), PanelState::default());
        let home = ws.active_panel();
        for i in 0..10 {
            ws.activate_panel(home).unwrap();
            ws.open_child(home, loc(&format!("/h/d{i}")), None).unwrap();
        }
        // The last opened (row 9) is active.
        let layout = compute(
            &ws,
            LayoutOptions {
                max_rows: 4,
                radius: 3,
            },
            None,
        );
        assert_eq!(layout.rows, 4);
        assert_eq!((layout.hidden_above, layout.hidden_below), (6, 0));
        assert_eq!(layout.active_row, 3);
        let h = cell(&layout, home);
        assert_eq!(
            (h.row, h.span),
            (0, 4),
            "the shared root is clipped to the window"
        );

        // Moving within the window keeps it still; leaving it moves it.
        ws.cycle_branch(Direction::Previous);
        let same = compute(
            &ws,
            LayoutOptions {
                max_rows: 4,
                radius: 3,
            },
            Some(layout.first_row),
        );
        assert_eq!(same.first_row, layout.first_row);
        for _ in 0..4 {
            ws.cycle_branch(Direction::Previous);
        }
        let moved = compute(
            &ws,
            LayoutOptions {
                max_rows: 4,
                radius: 3,
            },
            Some(layout.first_row),
        );
        assert!(moved.first_row < layout.first_row);
        assert!((0..moved.rows).contains(&moved.active_row));
    }

    #[test]
    fn one_row_is_the_single_strip() {
        let (ws, [home, _, _, downloads]) = scenario();
        let layout = compute(
            &ws,
            LayoutOptions {
                max_rows: 1,
                radius: 3,
            },
            None,
        );
        assert_eq!(layout.rows, 1);
        assert_eq!((layout.hidden_above, layout.hidden_below), (1, 0));
        let shown: Vec<PanelId> = layout.cells.iter().map(|c| c.panel).collect();
        assert_eq!(shown, [home, downloads]);
    }

    #[test]
    fn several_roots_stack_with_their_own_columns() {
        let (mut ws, _) = scenario();
        let usb = ws.open_root(loc("/media/usb"), PanelState::default()).panel;
        let layout = compute(&ws, ALL, None);
        let u = cell(&layout, usb);
        assert_eq!((u.column, u.row, u.span), (0, 2, 1));
        assert!(
            layout.edges.iter().all(|e| e.child != usb),
            "roots have no connector"
        );
    }

    #[test]
    fn inserting_a_parent_shifts_depths() {
        let mut ws = Workspace::new(loc("/h/Documents"), PanelState::default());
        let docs = ws.active_panel();
        let parent = ws.insert_parent(docs).unwrap();
        let layout = compute(&ws, ALL, None);
        assert_eq!(cell(&layout, parent).column, 0);
        assert_eq!(cell(&layout, docs).column, 1);
    }

    #[test]
    fn spans_match_branch_counts_on_random_workspaces() {
        let mut ws = Workspace::new(loc("/r"), PanelState::default());
        let mut seed = 7u64;
        let mut next = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 33) as usize) % n
        };
        for _ in 0..200 {
            let ids: Vec<PanelId> = ws.panels().map(|p| p.id()).collect();
            let from = ids[next(ids.len())];
            let location = ws
                .panel(from)
                .unwrap()
                .location()
                .child(format!("d{}", next(3)).as_ref());
            ws.open_child(from, location, None).unwrap();
        }
        let layout = compute(
            &ws,
            LayoutOptions {
                max_rows: 1000,
                radius: 3,
            },
            None,
        );
        assert_eq!(layout.rows, ws.branch_count());
        for c in &layout.cells {
            assert_eq!(
                c.span,
                ws.branches_through(c.panel).len(),
                "panel {}",
                c.panel
            );
            let path_rows: Vec<usize> = layout
                .branches
                .iter()
                .enumerate()
                .filter(|(_, b)| ws.path(**b).contains(&c.panel))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(path_rows, (c.row..c.row + c.span).collect::<Vec<_>>());
        }
    }

    /// Layout cost for growing workspaces (spatial spec §16.8). Run with
    /// `cargo test --release -p seshat-presentation layout_scaling -- --ignored --nocapture`.
    #[test]
    #[ignore = "benchmark"]
    fn layout_scaling() {
        for branches in [50, 200, 1000] {
            let mut ws = Workspace::new(loc("/w"), PanelState::default());
            let root = ws.active_panel();
            for i in 0..branches {
                ws.activate_panel(root).unwrap();
                let mut at = ws
                    .open_child(root, loc(&format!("/w/a{}", i % 10)), None)
                    .unwrap()
                    .panel;
                at = ws
                    .open_child(at, loc(&format!("/w/a{}/b{i}", i % 10)), None)
                    .unwrap()
                    .panel;
                for k in 0..3 {
                    let parent = ws.panel(at).unwrap().location().clone();
                    at = ws
                        .open_child(at, parent.child(format!("t{k}").as_ref()), None)
                        .unwrap()
                        .panel;
                }
            }
            let runs = 200;
            let started = std::time::Instant::now();
            for _ in 0..runs {
                std::hint::black_box(compute(
                    &ws,
                    LayoutOptions {
                        max_rows: 6,
                        radius: 3,
                    },
                    None,
                ));
            }
            println!(
                "layout, {branches:>4} branches, {:>4} panels: {:>10.2?} per call",
                ws.panel_count(),
                started.elapsed() / runs
            );
        }
    }
}
