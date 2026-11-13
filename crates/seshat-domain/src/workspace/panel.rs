use std::collections::HashSet;

use super::{BranchId, PanelId};
use crate::entities::{DirectoryIdentity, Location};
use crate::view::{ViewMode, ViewOptions};

/// Where a panel's list was scrolled to: an item and how far it was
/// scrolled past, in pixels. An item survives re-sorting and recycled rows;
/// a raw offset does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollAnchor {
    pub item: Location,
    pub offset: i32,
}

/// The files explicitly selected in one panel, in selection order, plus
/// the anchor that range selection extends from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    items: Vec<Location>,
    anchor: Option<Location>,
}

impl Selection {
    pub fn items(&self) -> &[Location] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, location: &Location) -> bool {
        self.items.contains(location)
    }

    pub fn anchor(&self) -> Option<&Location> {
        self.anchor.as_ref()
    }

    pub fn set_anchor(&mut self, anchor: Option<Location>) {
        self.anchor = anchor;
    }

    /// Adds or removes `location`; returns whether it is now selected.
    pub fn toggle(&mut self, location: Location) -> bool {
        if let Some(index) = self.items.iter().position(|l| *l == location) {
            self.items.remove(index);
            false
        } else {
            self.items.push(location);
            true
        }
    }

    pub fn insert(&mut self, location: Location) {
        if !self.contains(&location) {
            self.items.push(location);
        }
    }

    /// Replaces the selected items, keeping their order and dropping
    /// duplicates. The anchor is kept.
    pub fn replace(&mut self, items: impl IntoIterator<Item = Location>) {
        let mut seen = HashSet::new();
        self.items = items
            .into_iter()
            .filter(|l| seen.insert(l.clone()))
            .collect();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.anchor = None;
    }

    /// Drops items for which `keep` is false (e.g. files that no longer
    /// exist). Returns how many were dropped.
    pub fn retain(&mut self, mut keep: impl FnMut(&Location) -> bool) -> usize {
        let before = self.items.len();
        self.items.retain(&mut keep);
        if self.anchor.as_ref().is_some_and(|a| !keep(a)) {
            self.anchor = None;
        }
        before - self.items.len()
    }
}

/// Everything a panel remembers about how it was left. It belongs to the
/// workspace, not to any widget, so a panel can be hidden, virtualised or
/// re-rendered without losing it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelState {
    /// The navigation cursor (the "selected file"), if any.
    pub cursor: Option<Location>,
    pub scroll: Option<ScrollAnchor>,
    pub options: ViewOptions,
    pub mode: ViewMode,
    pub selection: Selection,
}

impl PanelState {
    /// The state a new panel inherits: the same presentation settings,
    /// but no search, cursor, scroll position or selection.
    pub fn inherit(&self) -> Self {
        Self {
            options: ViewOptions {
                query: String::new(),
                ..self.options.clone()
            },
            mode: self.mode,
            ..Self::default()
        }
    }
}

/// Why a panel's folder cannot be shown any more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailability {
    /// Deleted, moved or renamed away.
    Missing,
    /// The device it lived on was unmounted or unplugged.
    DeviceRemoved,
}

/// A persistent view of one directory, placed in the workspace tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    pub(super) id: PanelId,
    pub(super) location: Location,
    pub(super) parent: Option<PanelId>,
    pub(super) children: Vec<PanelId>,
    pub(super) unavailable: Option<Unavailability>,
    /// The folder's filesystem identity when last opened, if known.
    pub(super) identity: Option<DirectoryIdentity>,
    pub state: PanelState,
}

impl Panel {
    pub fn id(&self) -> PanelId {
        self.id
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn parent(&self) -> Option<PanelId> {
        self.parent
    }

    /// Child panels, in the order they were opened.
    pub fn children(&self) -> &[PanelId] {
        &self.children
    }

    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    pub fn unavailable(&self) -> Option<Unavailability> {
        self.unavailable
    }

    pub fn identity(&self) -> Option<DirectoryIdentity> {
        self.identity
    }
}

/// A navigation path through the panel tree, from a root to its `tip` (a
/// leaf). The path itself is derived from parent links, so a shared
/// ancestor exists once however many branches pass through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub(super) id: BranchId,
    pub(super) tip: PanelId,
    /// The panel this branch is "at": its active panel when it is the
    /// active branch. Always on the branch's path.
    pub(super) cursor: PanelId,
    // Logical times (workspace clock ticks), not wall-clock times.
    pub(super) created: u64,
    pub(super) last_active: u64,
}

impl Branch {
    pub fn id(&self) -> BranchId {
        self.id
    }

    pub fn tip(&self) -> PanelId {
        self.tip
    }

    pub fn cursor(&self) -> PanelId {
        self.cursor
    }

    pub fn created(&self) -> u64 {
        self.created
    }

    pub fn last_active(&self) -> u64 {
        self.last_active
    }
}
