use std::collections::HashMap;

use super::keys::{KeyPress, format_sequence, parse_sequence};

/// Everything a key binding can do. Keyboard, mouse, menus and the command
/// line all map onto these and the same application services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    PanelLeft,
    PanelRight,
    CursorDown,
    CursorUp,
    CursorFirst,
    CursorLast,
    HalfPageDown,
    HalfPageUp,
    Open,
    Parent,
    BranchPrevious,
    BranchNext,
    Back,
    Forward,
    /// `v`: enter Visual Mode (toggling the cursor item), or in Visual Mode
    /// toggle the cursor item.
    Visual,
    /// `V`: range selection from the cursor.
    VisualRange,
    CommandLine,
    Search,
    FocusMode,
    /// Esc: leave Visual Mode, or in Normal Mode clear the selection.
    Escape,
    Yank,
    Cut,
    Paste,
}

impl Action {
    pub const ALL: [Self; 23] = [
        Self::PanelLeft,
        Self::PanelRight,
        Self::CursorDown,
        Self::CursorUp,
        Self::CursorFirst,
        Self::CursorLast,
        Self::HalfPageDown,
        Self::HalfPageUp,
        Self::Open,
        Self::Parent,
        Self::BranchPrevious,
        Self::BranchNext,
        Self::Back,
        Self::Forward,
        Self::Visual,
        Self::VisualRange,
        Self::CommandLine,
        Self::Search,
        Self::FocusMode,
        Self::Escape,
        Self::Yank,
        Self::Cut,
        Self::Paste,
    ];

    /// Stable identifier used in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::PanelLeft => "panel-left",
            Self::PanelRight => "panel-right",
            Self::CursorDown => "cursor-down",
            Self::CursorUp => "cursor-up",
            Self::CursorFirst => "cursor-first",
            Self::CursorLast => "cursor-last",
            Self::HalfPageDown => "half-page-down",
            Self::HalfPageUp => "half-page-up",
            Self::Open => "open",
            Self::Parent => "parent",
            Self::BranchPrevious => "branch-previous",
            Self::BranchNext => "branch-next",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Visual => "visual",
            Self::VisualRange => "visual-range",
            Self::CommandLine => "command-line",
            Self::Search => "search",
            Self::FocusMode => "focus-mode",
            Self::Escape => "escape",
            Self::Yank => "yank",
            Self::Cut => "cut",
            Self::Paste => "paste",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::PanelLeft => "Previous panel",
            Self::PanelRight => "Next panel of the branch (without opening)",
            Self::CursorDown => "Move down",
            Self::CursorUp => "Move up",
            Self::CursorFirst => "First item",
            Self::CursorLast => "Last item",
            Self::HalfPageDown => "Scroll down",
            Self::HalfPageUp => "Scroll up",
            Self::Open => "Open: continue, return to or start a branch; files open",
            Self::Parent => "Parent panel",
            Self::BranchPrevious => "Previous branch",
            Self::BranchNext => "Next branch",
            Self::Back => "Back",
            Self::Forward => "Forward",
            Self::Visual => "Visual Selection Mode / toggle item",
            Self::VisualRange => "Range selection",
            Self::CommandLine => "Command Mode",
            Self::Search => "Search this panel",
            Self::FocusMode => "Toggle Focus Mode",
            Self::Escape => "Cancel / clear selection",
            Self::Yank => "Copy selection",
            Self::Cut => "Cut selection",
            Self::Paste => "Paste into this panel",
        }
    }

    /// Default bindings (spatial spec §4, §7.1; decisions D2, D9).
    fn defaults(self) -> &'static [&'static str] {
        match self {
            Self::PanelLeft => &["h"],
            Self::PanelRight => &["L"],
            Self::CursorDown => &["j"],
            Self::CursorUp => &["k"],
            Self::CursorFirst => &["gg"],
            Self::CursorLast => &["G"],
            Self::HalfPageDown => &["<C-d>"],
            Self::HalfPageUp => &["<C-u>"],
            // Context-aware navigation §3: `l` opens, and `Enter` is the same.
            Self::Open => &["l", "<CR>"],
            Self::Parent => &["<BS>"],
            Self::BranchPrevious => &["["],
            Self::BranchNext => &["]"],
            Self::Back => &["<C-o>"],
            Self::Forward => &["<C-i>"],
            Self::Visual => &["v"],
            Self::VisualRange => &["V"],
            Self::CommandLine => &[":"],
            Self::Search => &["/"],
            Self::FocusMode => &["zz"],
            Self::Escape => &["<Esc>"],
            Self::Yank => &["y"],
            Self::Cut => &["x"],
            Self::Paste => &["p"],
        }
    }

    /// Bindings made of one unmodified printable key. "Classic keys"
    /// (decision D1) turns these off so typing searches instead.
    fn is_single_letter(keys: &[KeyPress]) -> bool {
        keys.first()
            .is_some_and(|k| matches!(k.key, super::Key::Char(_)) && !k.ctrl && !k.alt)
    }
}

/// Result of looking up the keys typed so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Bound(Action),
    /// A longer binding starts with these keys; wait for more.
    Prefix,
    Unbound,
}

/// A validated set of key bindings.
#[derive(Debug, Clone)]
pub struct KeyMap {
    bindings: HashMap<Vec<KeyPress>, Action>,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::new(&[], false).0
    }
}

impl KeyMap {
    /// Builds the keymap from the defaults and user `overrides` (action id
    /// → key sequences; an empty list unbinds the action). Invalid or
    /// conflicting overrides are skipped and reported; the defaults for
    /// that action stay. With `classic`, single-key bindings are dropped.
    pub fn new(overrides: &[(String, Vec<String>)], classic: bool) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut table: HashMap<Action, Vec<Vec<KeyPress>>> = Action::ALL
            .into_iter()
            .map(|a| {
                let keys = a
                    .defaults()
                    .iter()
                    .map(|s| parse_sequence(s).expect("default bindings parse"))
                    .collect();
                (a, keys)
            })
            .collect();

        for (id, sequences) in overrides {
            let Some(action) = Action::from_id(id) else {
                warnings.push(format!("unknown action “{id}” in key bindings"));
                continue;
            };
            let parsed: Result<Vec<_>, _> = sequences.iter().map(|s| parse_sequence(s)).collect();
            match parsed {
                Ok(keys) => {
                    let previous = table.insert(action, keys);
                    if let Some(conflict) = find_conflict(&table) {
                        warnings.push(format!("binding for “{id}” ignored: {conflict}"));
                        table.insert(action, previous.unwrap_or_default());
                    }
                }
                Err(err) => warnings.push(format!("binding for “{id}” ignored: {err}")),
            }
        }

        let mut bindings = HashMap::new();
        for (action, sequences) in table {
            for keys in sequences {
                if classic && Action::is_single_letter(&keys) {
                    continue;
                }
                bindings.insert(keys, action);
            }
        }
        (Self { bindings }, warnings)
    }

    pub fn lookup(&self, keys: &[KeyPress]) -> Lookup {
        if let Some(action) = self.bindings.get(keys) {
            return Lookup::Bound(*action);
        }
        if self
            .bindings
            .keys()
            .any(|b| b.len() > keys.len() && b.starts_with(keys))
        {
            Lookup::Prefix
        } else {
            Lookup::Unbound
        }
    }

    /// The bindings of `action`, formatted (for help and tooltips).
    pub fn keys_for(&self, action: Action) -> Vec<String> {
        let mut keys: Vec<String> = self
            .bindings
            .iter()
            .filter(|(_, a)| **a == action)
            .map(|(k, _)| format_sequence(k))
            .collect();
        keys.sort();
        keys
    }
}

/// Two actions on one sequence, or one sequence a prefix of another, would
/// make a binding unreachable.
fn find_conflict(table: &HashMap<Action, Vec<Vec<KeyPress>>>) -> Option<String> {
    let all: Vec<(&Vec<KeyPress>, Action)> = table
        .iter()
        .flat_map(|(a, seqs)| seqs.iter().map(move |s| (s, *a)))
        .collect();
    for (i, (a, action_a)) in all.iter().enumerate() {
        for (b, action_b) in &all[i + 1..] {
            if a == b && action_a != action_b {
                return Some(format!(
                    "“{}” would be bound to both “{}” and “{}”",
                    format_sequence(a),
                    action_a.id(),
                    action_b.id()
                ));
            }
            if a.len() != b.len() && (a.starts_with(b) || b.starts_with(a)) {
                return Some(format!(
                    "“{}” and “{}” overlap",
                    format_sequence(a),
                    format_sequence(b)
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::keys::Key;

    fn keys(s: &str) -> Vec<KeyPress> {
        parse_sequence(s).unwrap()
    }

    #[test]
    fn defaults_follow_the_spec_and_do_not_conflict() {
        let map = KeyMap::default();
        assert_eq!(map.lookup(&keys("h")), Lookup::Bound(Action::PanelLeft));
        assert_eq!(map.lookup(&keys("l")), Lookup::Bound(Action::Open));
        assert_eq!(
            map.lookup(&keys("<CR>")),
            Lookup::Bound(Action::Open),
            "Enter ≡ l"
        );
        assert_eq!(map.lookup(&keys("L")), Lookup::Bound(Action::PanelRight));
        assert_eq!(map.lookup(&keys("gg")), Lookup::Bound(Action::CursorFirst));
        assert_eq!(map.lookup(&keys("g")), Lookup::Prefix);
        assert_eq!(map.lookup(&keys("zz")), Lookup::Bound(Action::FocusMode));
        assert_eq!(
            map.lookup(&keys("<C-d>")),
            Lookup::Bound(Action::HalfPageDown)
        );
        assert_eq!(
            map.lookup(&keys("[")),
            Lookup::Bound(Action::BranchPrevious)
        );
        assert_eq!(map.lookup(&keys("q")), Lookup::Unbound);
        let table: HashMap<Action, Vec<Vec<KeyPress>>> = Action::ALL
            .into_iter()
            .map(|a| (a, a.defaults().iter().map(|s| keys(s)).collect()))
            .collect();
        assert_eq!(find_conflict(&table), None);
    }

    #[test]
    fn overrides_rebind_and_unbind() {
        let (map, warnings) = KeyMap::new(
            &[
                ("focus-mode".into(), vec!["<C-f>".into()]),
                ("yank".into(), vec![]),
            ],
            false,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(map.lookup(&keys("<C-f>")), Lookup::Bound(Action::FocusMode));
        assert_eq!(map.lookup(&keys("zz")), Lookup::Unbound);
        assert_eq!(map.lookup(&keys("y")), Lookup::Unbound);
    }

    #[test]
    fn conflicting_or_invalid_overrides_are_rejected() {
        let (map, warnings) = KeyMap::new(
            &[
                ("focus-mode".into(), vec!["h".into()]),
                ("search".into(), vec!["g".into()]),
                ("bogus".into(), vec!["q".into()]),
                ("open".into(), vec!["<Nope>".into()]),
            ],
            false,
        );
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        // Defaults stay in force.
        assert_eq!(map.lookup(&keys("zz")), Lookup::Bound(Action::FocusMode));
        assert_eq!(map.lookup(&keys("/")), Lookup::Bound(Action::Search));
        assert_eq!(map.lookup(&keys("h")), Lookup::Bound(Action::PanelLeft));
    }

    #[test]
    fn classic_keys_drop_single_letter_bindings() {
        let (map, _) = KeyMap::new(&[], true);
        assert_eq!(map.lookup(&keys("h")), Lookup::Unbound);
        assert_eq!(map.lookup(&keys("g")), Lookup::Unbound);
        assert_eq!(
            map.lookup(&keys("<C-d>")),
            Lookup::Bound(Action::HalfPageDown)
        );
        assert_eq!(
            map.lookup(&[KeyPress::plain(Key::Enter)]),
            Lookup::Bound(Action::Open)
        );
    }

    #[test]
    fn action_ids_round_trip() {
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
        }
    }
}
