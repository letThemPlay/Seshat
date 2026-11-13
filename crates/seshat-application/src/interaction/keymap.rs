use std::collections::{HashMap, HashSet};
use std::fmt;

use super::keys::{KeyPress, format_sequence, parse_sequence};

seshat_domain::stable_ids! {
    /// The actions a key binding can do; the ids are used in the settings
    /// file. Menus, toolbar and commands use the wider `AppAction`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Action {
        PanelLeft => "panel-left",
        PanelRight => "panel-right",
        CursorDown => "cursor-down",
        CursorUp => "cursor-up",
        CursorFirst => "cursor-first",
        CursorLast => "cursor-last",
        HalfPageDown => "half-page-down",
        HalfPageUp => "half-page-up",
        Open => "open",
        Parent => "parent",
        BranchPrevious => "branch-previous",
        BranchNext => "branch-next",
        Back => "back",
        Forward => "forward",
        /// `v`: enter Visual Mode (toggling the cursor item), or in Visual Mode
        /// toggle the cursor item.
        Visual => "visual",
        /// `V`: range selection from the cursor.
        VisualRange => "visual-range",
        CommandLine => "command-line",
        Search => "search",
        FocusMode => "focus-mode",
        /// Esc: leave Visual Mode, or in Normal Mode clear the selection.
        Escape => "escape",
        Yank => "yank",
        Cut => "cut",
        Paste => "paste",
        /// `db`: prune the active branch back one panel. Workspace only; files
        /// are never touched (3-BACKWARD-BRANCH-PRUNING.md).
        PruneBack => "prune-back",
        /// `dB`: prune the active branch back to where it meets another branch.
        PruneBackSegment => "prune-back-segment",
        /// `u`: undo the last workspace change (a prune), not a file operation.
        UndoWorkspace => "undo-workspace",
        /// Ctrl+P: open Fuzzy Find over the workspace (Spec 4 §4.1).
        FuzzyFind => "fuzzy-find",
    }
}

impl Action {
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
            Self::PruneBack => "Prune the branch back one panel (files are not touched)",
            Self::PruneBackSegment => {
                "Prune the branch back to where it meets another branch (files are not touched)"
            }
            Self::UndoWorkspace => "Undo the last prune (not file operations)",
            Self::FuzzyFind => "Fuzzy Find: search for files and folders",
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
            // Backward pruning (decision B7): `d` is an operator prefix.
            Self::PruneBack => &["db"],
            Self::PruneBackSegment => &["dB"],
            Self::UndoWorkspace => &["u"],
            // Spec 4 §4.1; nothing else uses Ctrl+P.
            Self::FuzzyFind => &["<C-p>"],
        }
    }

    /// Actions whose defaults give way to an overlapping user binding
    /// instead of rejecting it (decision B8).
    fn defaults_yield_to_overrides(self) -> bool {
        matches!(
            self,
            Self::PruneBack | Self::PruneBackSegment | Self::UndoWorkspace | Self::FuzzyFind
        )
    }

    /// Bindings that start with an unmodified printable key, such as `j`
    /// or `gg`. "Classic keys" (decision D1) turns these off so typing
    /// searches instead.
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
    /// that action stay. With `classic`, bindings that start with a plain printable key are
    /// dropped.
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

        let mut overridden: HashSet<Action> = HashSet::new();
        for (id, sequences) in overrides {
            let Some(action) = Action::from_id(id) else {
                warnings.push(format!("unknown action “{id}” in key bindings"));
                continue;
            };
            let parsed: Result<Vec<_>, _> = sequences.iter().map(|s| parse_sequence(s)).collect();
            match parsed {
                Ok(keys) => {
                    let previous = table.insert(action, keys);
                    // Newer defaults overlapping this binding give way (B8).
                    let mut yielded: Vec<(Action, Vec<KeyPress>)> = Vec::new();
                    let rejected = loop {
                        let Some(conflict) = find_conflict(&table) else {
                            break None;
                        };
                        match conflict.yielding_default(action, &overridden) {
                            Some((other, keys)) => {
                                if let Some(sequences) = table.get_mut(&other) {
                                    sequences.retain(|s| *s != keys);
                                }
                                yielded.push((other, keys));
                            }
                            None => break Some(conflict),
                        }
                    };
                    if let Some(conflict) = rejected {
                        warnings.push(format!("binding for “{id}” ignored: {conflict}"));
                        table.insert(action, previous.unwrap_or_default());
                        for (other, keys) in yielded {
                            table.entry(other).or_default().push(keys);
                        }
                    } else {
                        overridden.insert(action);
                        for (other, keys) in yielded {
                            warnings.push(format!(
                                "default “{}” for “{}” dropped: it overlaps your binding for “{id}”",
                                format_sequence(&keys),
                                other.id()
                            ));
                        }
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

/// Two bindings that cannot both be reachable.
#[derive(Debug, PartialEq, Eq)]
struct Conflict {
    a: (Vec<KeyPress>, Action),
    b: (Vec<KeyPress>, Action),
}

impl Conflict {
    /// The binding to drop when `overriding` was just bound by the user:
    /// the other side, if it is a default that yields to overrides and has
    /// not itself been set by the user (decision B8).
    fn yielding_default(
        &self,
        overriding: Action,
        overridden: &HashSet<Action>,
    ) -> Option<(Action, Vec<KeyPress>)> {
        let other = match (self.a.1 == overriding, self.b.1 == overriding) {
            (true, false) => &self.b,
            (false, true) => &self.a,
            _ => return None,
        };
        (other.1.defaults_yield_to_overrides() && !overridden.contains(&other.1))
            .then(|| (other.1, other.0.clone()))
    }
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ((a, action_a), (b, action_b)) = (&self.a, &self.b);
        if a == b {
            write!(
                f,
                "“{}” would be bound to both “{}” and “{}”",
                format_sequence(a),
                action_a.id(),
                action_b.id()
            )
        } else {
            write!(
                f,
                "“{}” and “{}” overlap",
                format_sequence(a),
                format_sequence(b)
            )
        }
    }
}

/// Two actions on one sequence, or one sequence a prefix of another, would
/// make a binding unreachable.
fn find_conflict(table: &HashMap<Action, Vec<Vec<KeyPress>>>) -> Option<Conflict> {
    let all: Vec<(&Vec<KeyPress>, Action)> = table
        .iter()
        .flat_map(|(a, seqs)| seqs.iter().map(move |s| (s, *a)))
        .collect();
    for (i, (a, action_a)) in all.iter().enumerate() {
        for (b, action_b) in &all[i + 1..] {
            let same = a == b && action_a != action_b;
            let overlap = a.len() != b.len() && (a.starts_with(b) || b.starts_with(a));
            if same || overlap {
                return Some(Conflict {
                    a: ((*a).clone(), *action_a),
                    b: ((*b).clone(), *action_b),
                });
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
    fn prune_d_is_an_operator_prefix_and_u_undoes() {
        let map = KeyMap::default();
        assert_eq!(map.lookup(&keys("d")), Lookup::Prefix);
        assert_eq!(map.lookup(&keys("db")), Lookup::Bound(Action::PruneBack));
        assert_eq!(
            map.lookup(&keys("dB")),
            Lookup::Bound(Action::PruneBackSegment)
        );
        assert_eq!(map.lookup(&keys("u")), Lookup::Bound(Action::UndoWorkspace));
        assert_eq!(map.lookup(&keys("dd")), Lookup::Unbound);
        assert_eq!(
            map.lookup(&keys("<Del>")),
            Lookup::Unbound,
            "Delete stays with the Trash shortcut"
        );
    }

    #[test]
    fn prune_user_bindings_win_over_the_new_defaults() {
        let (map, warnings) = KeyMap::new(
            &[
                ("cut".into(), vec!["d".into()]),
                ("paste".into(), vec!["u".into()]),
            ],
            false,
        );
        assert_eq!(map.lookup(&keys("d")), Lookup::Bound(Action::Cut));
        assert_eq!(map.lookup(&keys("u")), Lookup::Bound(Action::Paste));
        assert!(map.keys_for(Action::PruneBack).is_empty());
        assert!(map.keys_for(Action::PruneBackSegment).is_empty());
        assert!(map.keys_for(Action::UndoWorkspace).is_empty());
        assert_eq!(
            warnings.len(),
            3,
            "each dropped default is reported: {warnings:?}"
        );

        // Rebinding pruning itself still frees `d` for the user binding.
        let (map, warnings) = KeyMap::new(
            &[
                ("prune-back".into(), vec!["<C-b>".into()]),
                ("cut".into(), vec!["d".into()]),
            ],
            false,
        );
        assert_eq!(map.lookup(&keys("<C-b>")), Lookup::Bound(Action::PruneBack));
        assert_eq!(map.lookup(&keys("d")), Lookup::Bound(Action::Cut));
        assert_eq!(warnings.len(), 1, "only dB was dropped: {warnings:?}");
    }

    #[test]
    fn a_user_binding_on_ctrl_p_wins_over_fuzzy_find() {
        assert_eq!(
            KeyMap::default().lookup(&keys("<C-p>")),
            Lookup::Bound(Action::FuzzyFind)
        );
        let (map, warnings) = KeyMap::new(&[("paste".into(), vec!["<C-p>".into()])], false);
        assert_eq!(map.lookup(&keys("<C-p>")), Lookup::Bound(Action::Paste));
        assert!(map.keys_for(Action::FuzzyFind).is_empty());
        assert_eq!(warnings.len(), 1, "the dropped default is reported");
    }

    #[test]
    fn prune_bindings_cannot_take_keys_from_established_actions() {
        let (map, warnings) = KeyMap::new(&[("prune-back".into(), vec!["y".into()])], false);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(map.lookup(&keys("y")), Lookup::Bound(Action::Yank));
        assert_eq!(map.lookup(&keys("db")), Lookup::Bound(Action::PruneBack));
    }

    #[test]
    fn action_ids_round_trip() {
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
        }
    }
}
