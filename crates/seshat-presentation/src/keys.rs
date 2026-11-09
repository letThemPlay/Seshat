//! Turns GDK key events into toolkit-independent [`KeyPress`]es and routes
//! them through the modal key handling of the application layer.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::prelude::*;
use seshat_application::interaction::{Intent, Interaction, Key, KeyMap, KeyPress, Mode};

/// Converts a key event. Returns `None` for keys bindings never use
/// (modifier keys on their own, function keys, Super combinations).
pub fn key_press(key: gdk::Key, state: gdk::ModifierType) -> Option<KeyPress> {
    if state.contains(gdk::ModifierType::SUPER_MASK) {
        return None;
    }
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    let alt = state.contains(gdk::ModifierType::ALT_MASK);
    let named = match key {
        gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => Some(Key::Enter),
        gdk::Key::Escape => Some(Key::Escape),
        gdk::Key::BackSpace => Some(Key::Backspace),
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => Some(Key::Tab),
        gdk::Key::Delete | gdk::Key::KP_Delete => Some(Key::Delete),
        gdk::Key::Up | gdk::Key::KP_Up => Some(Key::Up),
        gdk::Key::Down | gdk::Key::KP_Down => Some(Key::Down),
        gdk::Key::Left | gdk::Key::KP_Left => Some(Key::Left),
        gdk::Key::Right | gdk::Key::KP_Right => Some(Key::Right),
        gdk::Key::Home | gdk::Key::KP_Home => Some(Key::Home),
        gdk::Key::End | gdk::Key::KP_End => Some(Key::End),
        gdk::Key::Page_Up | gdk::Key::KP_Page_Up => Some(Key::PageUp),
        gdk::Key::Page_Down | gdk::Key::KP_Page_Down => Some(Key::PageDown),
        _ => None,
    };
    let key = match named {
        Some(key) => key,
        None => {
            let c = key.to_unicode().filter(|c| !c.is_control())?;
            // Ctrl+D arrives as `d` (or `D` with Shift); bindings use the
            // lowercase letter for Ctrl chords.
            Key::Char(if ctrl { c.to_ascii_lowercase() } else { c })
        }
    };
    Some(KeyPress { key, ctrl, alt })
}

/// Shared between the window's key controller, which must decide on the
/// spot whether a key is consumed, and the model, which acts on intents.
#[derive(Debug)]
pub struct KeyRouter {
    pub interaction: Interaction,
    pub keymap: KeyMap,
}

pub type SharedKeys = Rc<RefCell<KeyRouter>>;

impl KeyRouter {
    pub fn new(keymap: KeyMap) -> SharedKeys {
        Rc::new(RefCell::new(Self {
            interaction: Interaction::default(),
            keymap,
        }))
    }

    pub fn mode(&self) -> Mode {
        self.interaction.mode()
    }

    pub fn press(&mut self, key: KeyPress) -> Intent {
        self.interaction.press(key, &self.keymap)
    }
}

/// Whether keys should go to a text field (or a menu) rather than to the
/// workspace's key bindings.
pub fn typing_elsewhere(focus: Option<&gtk::Widget>) -> bool {
    focus.is_some_and(|w| {
        w.is::<gtk::Text>()
            || w.is::<gtk::Entry>()
            || w.is::<gtk::SearchEntry>()
            || w.is::<gtk::TextView>()
            || w.ancestor(gtk::Popover::static_type()).is_some()
    })
}
