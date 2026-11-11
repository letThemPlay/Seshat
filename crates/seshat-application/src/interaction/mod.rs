//! Keyboard-first interaction: Vim-style key notation, configurable key
//! bindings, the Normal / Visual / Command mode machine and the command
//! line. Pure logic, independent of the UI toolkit.

mod app_action;
pub mod command;
pub mod completion;
mod keymap;
mod keys;
mod mode;

pub use app_action::AppAction;
pub use keymap::{Action, KeyMap, Lookup};
pub use keys::{Key, KeyParseError, KeyPress, format_sequence, parse_sequence};
pub use mode::{Intent, Interaction, Mode};
