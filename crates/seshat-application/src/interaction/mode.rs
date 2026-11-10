use super::keymap::{Action, KeyMap, Lookup};
use super::keys::{Key, KeyPress};

/// The interaction mode. Focus Mode is not one of these: it is a
/// presentation state and works in every mode (spatial spec §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    /// Visual Selection Mode; `range` while extending a `V` range.
    Visual { range: bool },
    /// The command line has keyboard focus.
    Command,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Visual { range: false } => "VISUAL",
            Self::Visual { range: true } => "VISUAL RANGE",
            Self::Command => "COMMAND",
        }
    }
}

/// What a key press amounts to, after mode rules are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// The first keys of a longer binding; nothing to do yet.
    Pending,
    /// Not a binding: let the toolkit handle the key (arrows, Ctrl+C…).
    Unhandled,
    /// A partial sequence was abandoned.
    Cancelled,
    /// Move the cursor; in a range selection, extend the range.
    MoveCursor {
        delta: isize,
        extend: bool,
    },
    CursorFirst {
        extend: bool,
    },
    CursorLast {
        extend: bool,
    },
    /// Half a page; the caller knows the page size.
    HalfPage {
        down: bool,
        extend: bool,
    },
    ToggleSelection,
    StartRange,
    /// `V` again: stop extending the range, keep the selection.
    EndRange,
    /// Leaving Visual Mode: the selection is kept.
    LeaveVisual,
    ClearSelection,
    OpenCommandLine,
    /// Open the cursor item (`l`, `Enter`). Folders always navigate; files
    /// are launched only outside Visual Mode, so selecting never opens
    /// files (decision C6).
    Open {
        launch_files: bool,
    },
    /// Everything else, carried out the same way in every mode.
    Perform(Action),
}

/// Modal key handling: pending multi-key sequences and mode transitions.
/// Pure logic; the presentation layer feeds it key presses.
#[derive(Debug, Default)]
pub struct Interaction {
    mode: Mode,
    pending: Vec<KeyPress>,
}

impl Interaction {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Keys typed so far of an unfinished sequence (e.g. `g`).
    pub fn pending(&self) -> &[KeyPress] {
        &self.pending
    }

    /// The command line closed (run or cancelled): back to Normal Mode.
    pub fn leave_command_line(&mut self) {
        if self.mode == Mode::Command {
            self.mode = Mode::Normal;
        }
    }

    /// Back to Normal Mode from anywhere (e.g. the selection was used up).
    pub fn reset(&mut self) {
        self.mode = Mode::Normal;
        self.pending.clear();
    }

    pub fn press(&mut self, key: KeyPress, keymap: &KeyMap) -> Intent {
        if self.mode == Mode::Command {
            return Intent::Unhandled; // the command line's text field has it
        }
        if key.key == Key::Escape && !key.ctrl && !key.alt && !self.pending.is_empty() {
            self.pending.clear();
            return Intent::Cancelled;
        }
        self.pending.push(key);
        match keymap.lookup(&self.pending) {
            Lookup::Prefix => Intent::Pending,
            Lookup::Bound(action) => {
                self.pending.clear();
                self.apply(action)
            }
            Lookup::Unbound => {
                let abandoned = self.pending.len() > 1;
                self.pending.clear();
                if abandoned {
                    Intent::Cancelled
                } else {
                    Intent::Unhandled
                }
            }
        }
    }

    /// Applies `action` in the current mode. Also used for actions that do
    /// not come from keys (menus, buttons).
    pub fn apply(&mut self, action: Action) -> Intent {
        let range = matches!(self.mode, Mode::Visual { range: true });
        match (self.mode, action) {
            (_, Action::CursorDown) => Intent::MoveCursor {
                delta: 1,
                extend: range,
            },
            (_, Action::CursorUp) => Intent::MoveCursor {
                delta: -1,
                extend: range,
            },
            (_, Action::CursorFirst) => Intent::CursorFirst { extend: range },
            (_, Action::CursorLast) => Intent::CursorLast { extend: range },
            (_, Action::HalfPageDown) => Intent::HalfPage {
                down: true,
                extend: range,
            },
            (_, Action::HalfPageUp) => Intent::HalfPage {
                down: false,
                extend: range,
            },
            (_, Action::Visual) => {
                self.mode = Mode::Visual { range: false };
                Intent::ToggleSelection
            }
            (Mode::Visual { range: true }, Action::VisualRange) => {
                self.mode = Mode::Visual { range: false };
                Intent::EndRange
            }
            (_, Action::VisualRange) => {
                self.mode = Mode::Visual { range: true };
                Intent::StartRange
            }
            (Mode::Visual { .. }, Action::Escape) => {
                self.mode = Mode::Normal;
                Intent::LeaveVisual
            }
            (_, Action::Escape) => Intent::ClearSelection,
            (_, Action::CommandLine) => {
                self.mode = Mode::Command;
                Intent::OpenCommandLine
            }
            (mode, Action::Open) => Intent::Open {
                launch_files: !matches!(mode, Mode::Visual { .. }),
            },
            (_, other) => Intent::Perform(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::keys::parse_sequence;

    fn type_keys(i: &mut Interaction, map: &KeyMap, text: &str) -> Vec<Intent> {
        parse_sequence(text)
            .unwrap()
            .into_iter()
            .map(|k| i.press(k, map))
            .collect()
    }

    #[test]
    fn spec_16_5_normal_mode_is_the_default() {
        assert_eq!(Interaction::default().mode(), Mode::Normal);
    }

    #[test]
    fn multi_key_sequences_wait_then_fire() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        assert_eq!(
            type_keys(&mut i, &map, "gg"),
            [Intent::Pending, Intent::CursorFirst { extend: false }]
        );
        assert_eq!(
            type_keys(&mut i, &map, "zz"),
            [Intent::Pending, Intent::Perform(Action::FocusMode)]
        );
        assert_eq!(
            type_keys(&mut i, &map, "gq"),
            [Intent::Pending, Intent::Cancelled]
        );
        assert!(i.pending().is_empty());
        assert_eq!(
            type_keys(&mut i, &map, "g<Esc>"),
            [Intent::Pending, Intent::Cancelled]
        );
    }

    #[test]
    fn spec_16_5_escape_is_consistent() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        // Normal: Esc clears the selection.
        assert_eq!(type_keys(&mut i, &map, "<Esc>"), [Intent::ClearSelection]);
        // Visual: Esc leaves Visual Mode and keeps the selection.
        type_keys(&mut i, &map, "v");
        assert_eq!(type_keys(&mut i, &map, "<Esc>"), [Intent::LeaveVisual]);
        assert_eq!(i.mode(), Mode::Normal);
    }

    #[test]
    fn spec_16_5_visual_mode_supports_range_selection() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        assert_eq!(type_keys(&mut i, &map, "V"), [Intent::StartRange]);
        assert_eq!(i.mode(), Mode::Visual { range: true });
        assert_eq!(
            type_keys(&mut i, &map, "jk"),
            [
                Intent::MoveCursor {
                    delta: 1,
                    extend: true
                },
                Intent::MoveCursor {
                    delta: -1,
                    extend: true
                }
            ]
        );
        // v toggles individual items and ends the range.
        assert_eq!(type_keys(&mut i, &map, "v"), [Intent::ToggleSelection]);
        assert_eq!(i.mode(), Mode::Visual { range: false });
        assert_eq!(
            type_keys(&mut i, &map, "j"),
            [Intent::MoveCursor {
                delta: 1,
                extend: false
            }]
        );
    }

    #[test]
    fn visual_mode_keeps_panel_and_branch_navigation() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        type_keys(&mut i, &map, "v");
        assert_eq!(
            type_keys(&mut i, &map, "hL]"),
            [
                Intent::Perform(Action::PanelLeft),
                Intent::Perform(Action::PanelRight),
                Intent::Perform(Action::BranchNext)
            ]
        );
        assert_eq!(i.mode(), Mode::Visual { range: false });
    }

    #[test]
    fn nav_l_and_enter_open_identically_and_visual_mode_never_launches_files() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        let normal = Intent::Open { launch_files: true };
        assert_eq!(type_keys(&mut i, &map, "l<CR>"), [normal, normal]);
        type_keys(&mut i, &map, "v");
        let visual = Intent::Open {
            launch_files: false,
        };
        assert_eq!(type_keys(&mut i, &map, "l<CR>"), [visual, visual]);
        assert_eq!(
            i.mode(),
            Mode::Visual { range: false },
            "opening keeps the mode"
        );
    }

    #[test]
    fn spec_16_5_command_mode_input_is_not_routed_to_navigation() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        assert_eq!(type_keys(&mut i, &map, ":"), [Intent::OpenCommandLine]);
        assert_eq!(i.mode(), Mode::Command);
        assert_eq!(
            type_keys(&mut i, &map, "hjkl"),
            [Intent::Unhandled; 4],
            "typed text belongs to the command line"
        );
        i.leave_command_line();
        assert_eq!(i.mode(), Mode::Normal);
    }

    #[test]
    fn focus_mode_does_not_change_the_interaction_mode() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        type_keys(&mut i, &map, "V");
        assert_eq!(
            type_keys(&mut i, &map, "zz"),
            [Intent::Pending, Intent::Perform(Action::FocusMode)]
        );
        assert_eq!(i.mode(), Mode::Visual { range: true });
    }

    #[test]
    fn unbound_keys_are_left_to_the_toolkit() {
        let map = KeyMap::default();
        let mut i = Interaction::default();
        assert_eq!(type_keys(&mut i, &map, "<Down>"), [Intent::Unhandled]);
        assert_eq!(type_keys(&mut i, &map, "<C-c>"), [Intent::Unhandled]);
    }
}
