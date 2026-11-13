//! Modal dialogs: conflicts, names, confirmations.
//!
//! Dialogs carry the `seshat` class so the active theme styles them.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use seshat_application::operations::validate_name;
use seshat_domain::operation::{Conflict, ConflictDecision, ConflictResolution, EntryType};

fn dialog_window(parent: &gtk::Window, title: &str) -> gtk::Window {
    let window = gtk::Window::builder()
        .title(title)
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .default_width(440)
        .css_classes(["seshat", "seshat-dialog"])
        // A dialog left open must not keep the app alive after the main
        // window closes.
        .destroy_with_parent(true)
        .build();
    // Escape dismisses, like closing the window (conflicts: Cancel).
    let escape = gtk::ShortcutController::new();
    escape.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape"),
        Some(gtk::NamedAction::new("window.close")),
    ));
    window.add_controller(escape);
    window
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn button_row(buttons: &[&gtk::Button]) -> gtk::Box {
    let row = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::End)
        .margin_top(12)
        .build();
    for button in buttons {
        row.append(*button);
    }
    row
}

fn content_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build()
}

type Answer = Rc<RefCell<Option<Box<dyn FnOnce(ConflictDecision)>>>>;

/// Asks how to resolve a conflict. `on_answer` is called once (Cancel if
/// the window is closed). Returns the window so the caller can
/// close it if the operation ends first.
pub fn conflict(
    parent: &gtk::Window,
    conflict: &Conflict,
    on_answer: impl FnOnce(ConflictDecision) + 'static,
) -> gtk::Window {
    let window = dialog_window(parent, "Item already exists");
    let content = content_box();

    let what = |t: EntryType| {
        if t == EntryType::Directory {
            "folder"
        } else {
            "file"
        }
    };
    let folder = conflict.target.parent().map(name_of).unwrap_or_default();
    let heading = gtk::Label::builder()
        .label(format!(
            "A {} named “{}” already exists in “{}”.",
            what(conflict.target_type),
            name_of(&conflict.target),
            folder
        ))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["title-4"])
        .build();
    let merge = conflict.source_type == EntryType::Directory
        && conflict.target_type == EntryType::Directory;
    let body = gtk::Label::builder()
        .label(if merge {
            "Merging adds the incoming items to the existing folder; files with the same name are asked about individually."
        } else {
            "Replacing overwrites its contents. This cannot be undone."
        })
        .wrap(true)
        .xalign(0.0)
        .build();
    let apply_all = gtk::CheckButton::with_label("Apply this choice to all conflicts");

    let cancel = gtk::Button::with_label("Cancel Operation");
    let skip = gtk::Button::with_label("Skip");
    let rename = gtk::Button::with_label("Keep Both");
    let replace = gtk::Button::with_label(if merge { "Merge" } else { "Replace" });
    replace.add_css_class("destructive-action");
    skip.add_css_class("suggested-action");

    content.append(&heading);
    content.append(&body);
    content.append(&apply_all);
    content.append(&button_row(&[&cancel, &skip, &rename, &replace]));
    window.set_child(Some(&content));
    window.set_default_widget(Some(&skip));

    // Taken by the first answer, so later clicks or the close request
    // find it gone.
    let on_answer: Answer = Rc::new(RefCell::new(Some(Box::new(on_answer))));
    let respond = {
        let window = window.clone();
        let on_answer = on_answer.clone();
        move |resolution| {
            let Some(on_answer) = on_answer.take() else {
                return;
            };
            on_answer(ConflictDecision {
                resolution,
                apply_to_all: apply_all.is_active(),
            });
            window.close();
        }
    };
    for (button, resolution) in [
        (&cancel, ConflictResolution::Cancel),
        (&skip, ConflictResolution::Skip),
        (&rename, ConflictResolution::Rename),
        (&replace, ConflictResolution::Overwrite),
    ] {
        let respond = respond.clone();
        button.connect_clicked(move |_| respond(resolution));
    }
    window.connect_close_request(move |_| {
        if let Some(on_answer) = on_answer.take() {
            on_answer(ConflictDecision {
                resolution: ConflictResolution::Cancel,
                apply_to_all: false,
            });
        }
        gtk::glib::Propagation::Proceed
    });
    window.present();
    skip.grab_focus();
    window
}

/// Asks for a file name, validating as the user types.
pub fn name_prompt(
    parent: &gtk::Window,
    title: &str,
    confirm: &str,
    initial: &str,
    select_stem: bool,
    on_ok: impl Fn(String) + 'static,
) {
    let window = dialog_window(parent, title);
    let content = content_box();
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .css_classes(["title-4"])
        .build();
    let entry = gtk::Entry::builder()
        .text(initial)
        .activates_default(true)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(title)]);
    let error = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["seshat-error"])
        .build();
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label(confirm);
    ok.add_css_class("suggested-action");

    content.append(&heading);
    content.append(&entry);
    content.append(&error);
    content.append(&button_row(&[&cancel, &ok]));
    window.set_child(Some(&content));
    window.set_default_widget(Some(&ok));

    let validate = {
        let ok = ok.clone();
        let error = error.clone();
        move |text: &str| {
            let result = validate_name(text);
            ok.set_sensitive(result.is_ok());
            error.set_label(&result.err().map(|e| e.to_string()).unwrap_or_default());
        }
    };
    validate(initial);
    entry.connect_changed(move |e| validate(&e.text()));

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        ok.connect_clicked(move |_| {
            let text = entry.text().to_string();
            if validate_name(&text).is_ok() {
                on_ok(text);
                window.close();
            }
        });
    }
    window.present();
    entry.grab_focus();
    // Select the name without its extension, like other file managers.
    let stem_end = if select_stem {
        initial
            .rfind('.')
            .filter(|&i| i > 0)
            .map_or(-1, |i| initial[..i].chars().count() as i32)
    } else {
        -1
    };
    entry.select_region(0, stem_end);
}

/// Asks to confirm a destructive action.
pub fn confirm(
    parent: &gtk::Window,
    heading: &str,
    body: &str,
    action: &str,
    on_confirm: impl Fn() + 'static,
) {
    confirm_styled(parent, heading, body, action, true, on_confirm);
}

/// Asks to confirm an action that is not destructive (e.g. a copy).
pub fn confirm_safe(
    parent: &gtk::Window,
    heading: &str,
    body: &str,
    action: &str,
    on_confirm: impl Fn() + 'static,
) {
    confirm_styled(parent, heading, body, action, false, on_confirm);
}

fn confirm_styled(
    parent: &gtk::Window,
    heading: &str,
    body: &str,
    action: &str,
    destructive: bool,
    on_confirm: impl Fn() + 'static,
) {
    let window = dialog_window(parent, heading);
    let content = content_box();
    content.append(
        &gtk::Label::builder()
            .label(heading)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["title-4"])
            .build(),
    );
    content.append(
        &gtk::Label::builder()
            .label(body)
            .wrap(true)
            .xalign(0.0)
            .build(),
    );
    let cancel = gtk::Button::with_label("Cancel");
    let go = gtk::Button::with_label(action);
    go.add_css_class(if destructive {
        "destructive-action"
    } else {
        "suggested-action"
    });
    content.append(&button_row(&[&cancel, &go]));
    window.set_child(Some(&content));
    // The safe choice is the default.
    window.set_default_widget(Some(&cancel));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        go.connect_clicked(move |_| {
            on_confirm();
            window.close();
        });
    }
    window.present();
    cancel.grab_focus();
}

/// Picks one of `options` (label and location), e.g. the destination of
/// `:copy` among the open panels. Nothing happens if the window is closed.
pub fn choose(
    parent: &gtk::Window,
    heading: &str,
    options: &[(String, seshat_domain::entities::Location)],
    on_pick: impl Fn(seshat_domain::entities::Location) + 'static,
) {
    let window = dialog_window(parent, heading);
    window.set_resizable(true);
    window.set_default_height(360);
    let content = content_box();
    content.append(
        &gtk::Label::builder()
            .label(heading)
            .xalign(0.0)
            .css_classes(["title-4"])
            .build(),
    );
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(false)
        .build();
    list.update_property(&[gtk::accessible::Property::Label("Destinations")]);
    for (label, _) in options {
        list.append(
            &gtk::Label::builder()
                .label(label.as_str())
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .margin_top(4)
                .margin_bottom(4)
                .build(),
        );
    }
    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .min_content_height(200)
        .build();
    content.append(&scroll);
    let cancel = gtk::Button::with_label("Cancel");
    let go = gtk::Button::with_label("Choose");
    go.add_css_class("suggested-action");
    content.append(&button_row(&[&cancel, &go]));
    window.set_child(Some(&content));

    let locations: Vec<_> = options.iter().map(|(_, l)| l.clone()).collect();
    let on_pick = Rc::new(on_pick);
    let pick = {
        let window = window.clone();
        let list = list.clone();
        move || {
            if let Some(location) = list
                .selected_row()
                .and_then(|r| usize::try_from(r.index()).ok())
                .and_then(|i| locations.get(i).cloned())
            {
                on_pick(location);
                window.close();
            }
        }
    };
    let pick = Rc::new(pick);
    {
        let pick = pick.clone();
        go.connect_clicked(move |_| pick());
    }
    list.connect_row_activated(move |_, _| pick());
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }
    window.present();
    list.grab_focus();
}

/// Shows read-only text, e.g. key bindings and commands.
pub fn info(parent: &gtk::Window, heading: &str, text: &str) {
    let window = dialog_window(parent, heading);
    window.set_resizable(true);
    window.set_default_size(560, 480);
    let content = content_box();
    content.append(
        &gtk::Label::builder()
            .label(heading)
            .xalign(0.0)
            .css_classes(["title-4"])
            .build(),
    );
    let body = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .yalign(0.0)
        .selectable(true)
        .css_classes(["monospace"])
        .build();
    content.append(
        &gtk::ScrolledWindow::builder()
            .child(&body)
            .vexpand(true)
            .min_content_height(300)
            .build(),
    );
    let close = gtk::Button::with_label("Close");
    content.append(&button_row(&[&close]));
    window.set_child(Some(&content));
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }
    window.present();
    close.grab_focus();
}
