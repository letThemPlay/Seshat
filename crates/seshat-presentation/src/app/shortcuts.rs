//! Keyboard shortcuts and mouse navigation buttons.

use gtk::prelude::*;
use relm4::ComponentSender;

use super::{App, CommandLineMsg, Msg};
use crate::keys::{self, SharedKeys};
use seshat_application::interaction::{Action, AppAction, Intent, Mode};
use seshat_application::view::ViewMode;
use seshat_application::workspace::file_actions::ItemKind;

/// GTK accelerators and the window actions they carry out, always.
const SHORTCUTS: &[(&str, AppAction)] = &[
    ("<Alt>Left", AppAction::Key(Action::Back)),
    ("Back", AppAction::Key(Action::Back)),
    ("<Alt>Right", AppAction::Key(Action::Forward)),
    ("Forward", AppAction::Key(Action::Forward)),
    ("<Alt>Up", AppAction::Key(Action::Parent)),
    ("F5", AppAction::Refresh),
    ("<Control>r", AppAction::Refresh),
    ("<Control>h", AppAction::ToggleHidden),
    ("<Control>l", AppAction::EditAddress),
    ("<Control>f", AppAction::Key(Action::Search)),
    ("<Control>1", AppAction::SetViewMode(ViewMode::List)),
    ("<Control>2", AppAction::SetViewMode(ViewMode::Grid)),
    ("<Control><Shift>c", AppAction::CopyPaths),
    ("<Shift>F10", AppAction::KeyboardContextMenu),
    ("Menu", AppAction::KeyboardContextMenu),
];

/// Shortcuts that go through the window's actions, so they are ignored
/// while their action is disabled (e.g. Rename with no selection).
const ACTION_SHORTCUTS: &[(&str, AppAction)] = &[
    ("<Control>x", AppAction::Key(Action::Cut)),
    ("<Control>c", AppAction::Key(Action::Yank)),
    ("<Control>v", AppAction::Key(Action::Paste)),
    ("F2", AppAction::Rename),
    ("Delete", AppAction::Trash),
    ("KP_Delete", AppAction::Trash),
    ("<Shift>Delete", AppAction::Delete),
    ("<Control><Shift>n", AppAction::NewItem(ItemKind::Folder)),
];

pub fn install(
    window: &gtk::Window,
    router: &SharedKeys,
    command_entry: &gtk::Entry,
    sender: &ComponentSender<App>,
) {
    install_modal_keys(window, router, command_entry, sender);
    let controller = gtk::ShortcutController::new();
    for &(accelerator, action) in ACTION_SHORTCUTS {
        let Some(trigger) = gtk::ShortcutTrigger::parse_string(accelerator) else {
            tracing::warn!(accelerator, "invalid shortcut trigger");
            continue;
        };
        let action = gtk::NamedAction::new(&crate::actions::detailed_name(action));
        controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
    }
    for &(accelerator, action) in SHORTCUTS {
        let Some(trigger) = gtk::ShortcutTrigger::parse_string(accelerator) else {
            tracing::warn!(accelerator, "invalid shortcut trigger");
            continue;
        };
        let sender = sender.clone();
        let action = gtk::CallbackAction::new(move |_, _| {
            sender.input_sender().emit(Msg::Perform(action));
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
        8 => sender
            .input_sender()
            .emit(Msg::Perform(Action::Back.into())),
        9 => sender
            .input_sender()
            .emit(Msg::Perform(Action::Forward.into())),
        _ => {}
    });
    window.add_controller(click);
}

/// Key bindings of the workspace (h/j/k/l, gg, zz, v, :, …), resolved by
/// the modal key handling before any widget sees the key. Keys that are
/// not bindings, and all keys while typing in a text field or menu, go on
/// to GTK as usual, so arrows, Ctrl shortcuts and dialogs keep working.
fn install_modal_keys(
    window: &gtk::Window,
    router: &SharedKeys,
    command_entry: &gtk::Entry,
    sender: &ComponentSender<App>,
) {
    {
        // Leaving the window abandons a half-typed sequence (e.g. the `d`
        // of `db`), so it cannot complete in a context the user no longer
        // sees. Re-render so the pending indicator clears.
        let router = router.clone();
        let sender = sender.clone();
        window.connect_is_active_notify(move |window| {
            if !window.is_active() && router.borrow_mut().interaction.cancel_pending() {
                sender.input_sender().emit(Msg::Intent(Intent::Cancelled));
            }
        });
    }
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let router = router.clone();
    let sender = sender.clone();
    let window_ref = window.downgrade();
    let entry = command_entry.clone();
    controller.connect_key_pressed(move |_, key, _, state| {
        let Some(window) = window_ref.upgrade() else {
            return gtk::glib::Propagation::Proceed;
        };
        let focus = gtk::prelude::GtkWindowExt::focus(&window);
        if keys::typing_elsewhere(focus.as_ref()) {
            return gtk::glib::Propagation::Proceed;
        }
        let Some(press) = keys::key_press(key, state) else {
            return gtk::glib::Propagation::Proceed;
        };
        // In Command Mode keys belong to the command line. If it lost focus
        // (a click elsewhere), the prompt was abandoned: close it and treat
        // this key as a Normal Mode key.
        if router.borrow().mode() == Mode::Command {
            router.borrow_mut().interaction.leave_command_line();
            entry.set_visible(false);
            sender
                .input_sender()
                .emit(Msg::CommandLine(CommandLineMsg::Cancelled));
        }
        let intent = router.borrow_mut().press(press);
        if intent == Intent::OpenCommandLine {
            // Focus the prompt now, so the very next key already lands in it.
            entry.set_text("");
            entry.set_visible(true);
            entry.grab_focus();
        }
        tracing::trace!(
            key = %press,
            ?intent,
            focus = focus.as_ref().map(|w| w.type_().name()).unwrap_or("none"),
            "key"
        );
        if intent == Intent::Unhandled {
            return gtk::glib::Propagation::Proceed;
        }
        sender.input_sender().emit(Msg::Intent(intent));
        gtk::glib::Propagation::Stop
    });
    window.add_controller(controller);
}
