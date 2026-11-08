//! Keyboard shortcuts and mouse navigation buttons.

use gtk::prelude::*;
use relm4::ComponentSender;

use super::{App, Msg};

/// A GTK accelerator and the message it sends.
type Shortcut = (&'static str, fn() -> Msg);

const SHORTCUTS: &[Shortcut] = &[
    ("<Alt>Left", || Msg::Back),
    ("Back", || Msg::Back),
    ("<Alt>Right", || Msg::Forward),
    ("Forward", || Msg::Forward),
    ("<Alt>Up", || Msg::GoUp),
    ("F5", || Msg::Refresh),
    ("<Control>r", || Msg::Refresh),
    ("<Control>h", || Msg::ToggleHidden),
    ("<Control>l", || Msg::EditAddress),
    ("<Control>f", || Msg::StartSearch),
    ("<Control>1", || Msg::SetViewMode("list".to_owned())),
    ("<Control>2", || Msg::SetViewMode("grid".to_owned())),
    ("<Control><Shift>c", || Msg::CopyPaths),
    ("<Shift>F10", || Msg::KeyboardContextMenu),
    ("Menu", || Msg::KeyboardContextMenu),
];

/// Shortcuts that trigger actions, so disabled actions (e.g. Rename with no
/// selection) are ignored.
const ACTION_SHORTCUTS: &[(&str, &str)] = &[
    ("<Control>x", "cut"),
    ("<Control>c", "copy"),
    ("<Control>v", "paste"),
    ("F2", "rename"),
    ("Delete", "trash"),
    ("KP_Delete", "trash"),
    ("<Shift>Delete", "delete"),
    ("<Control><Shift>n", "new-folder"),
];

pub fn install(window: &gtk::Window, sender: &ComponentSender<App>) {
    let controller = gtk::ShortcutController::new();
    for (accelerator, action) in ACTION_SHORTCUTS {
        if let Some(trigger) = gtk::ShortcutTrigger::parse_string(accelerator) {
            let action = gtk::NamedAction::new(&format!("{}.{action}", crate::actions::GROUP));
            controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
        }
    }
    for (accelerator, message) in SHORTCUTS {
        let Some(trigger) = gtk::ShortcutTrigger::parse_string(accelerator) else {
            tracing::warn!(accelerator, "invalid shortcut trigger");
            continue;
        };
        let sender = sender.clone();
        let message = *message;
        let action = gtk::CallbackAction::new(move |_, _| {
            sender.input_sender().emit(message());
            gtk::glib::Propagation::Stop
        });
        controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
    }
    window.add_controller(controller);

    // Mouse "back" (8) and "forward" (9) buttons.
    let click = gtk::GestureClick::builder()
        .button(0)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let sender = sender.clone();
    click.connect_pressed(move |gesture, _, _, _| match gesture.current_button() {
        8 => sender.input_sender().emit(Msg::Back),
        9 => sender.input_sender().emit(Msg::Forward),
        _ => {}
    });
    window.add_controller(click);
}
