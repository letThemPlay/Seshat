//! The Fuzzy Find overlay's widgets (Spec 4 §4.2, §7): the search field,
//! the scope and what it covers, a status line and a virtualised result
//! list with the matched characters highlighted. It only draws and
//! reports input; the controller (`app::fuzzy_find`) decides everything.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};
use seshat_application::preferences::ScopeChoice;
use seshat_domain::search::SearchResult;

/// Rows moved by Page Up and Page Down.
const PAGE: isize = 10;

/// A scope's full name, as screen readers and announcements say it, and
/// the short one on its button.
fn scope_names(scope: ScopeChoice) -> (&'static str, &'static str) {
    match scope {
        ScopeChoice::CurrentDirectory => ("Current Directory", "Directory"),
        ScopeChoice::CurrentBranch => ("Current Branch", "Branch"),
        ScopeChoice::EntireWorkspace => ("Entire Workspace", "Workspace"),
    }
}

/// A scope's full name, as screen readers and announcements say it.
pub fn scope_name(scope: ScopeChoice) -> &'static str {
    scope_names(scope).0
}

/// A scope's button: its number, which is also its `Alt+` key, and its
/// short name.
fn scope_button_label(scope: ScopeChoice) -> String {
    format!("{} {}", scope.number(), scope_names(scope).1)
}

/// What the user did in the overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FuzzyViewEvent {
    /// The search text changed.
    Changed(String),
    /// Up, Down, Page Up or Page Down: move the highlight by this much.
    Move(isize),
    /// A result was clicked: highlight it.
    Highlight(u32),
    /// Enter, or a double-click: open the highlighted result.
    Activate,
    /// Alt+Enter: show the highlighted result in its folder.
    Reveal,
    /// A scope was picked with its button or Alt+1…3.
    Scope(ScopeChoice),
    /// Tab (1) or Shift+Tab (-1): the next or previous scope (F14).
    CycleScope(isize),
    /// Esc.
    Close,
}

/// A result as listed: the result and, when several branches are
/// searched, the number of the branch it was found in (§6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    pub result: SearchResult,
    pub branch: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct FuzzyView {
    pub root: gtk::Box,
    entry: gtk::SearchEntry,
    /// The scope buttons, in the order of [`ScopeChoice::ALL`].
    scopes: gtk::Box,
    scope_buttons: Vec<gtk::ToggleButton>,
    /// The folders the scope covers.
    scope: gtk::Label,
    status: gtk::Label,
    store: gio::ListStore,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    scroller: gtk::ScrolledWindow,
    /// Set while the model changes the highlight, so the selection change
    /// it causes isn't taken for a click.
    updating: Rc<Cell<bool>>,
}

impl FuzzyView {
    pub fn new(on_event: impl Fn(FuzzyViewEvent) + 'static) -> Self {
        let on_event: Rc<dyn Fn(FuzzyViewEvent)> = Rc::new(on_event);
        let entry = gtk::SearchEntry::builder()
            .placeholder_text("Find files and folders")
            .hexpand(true)
            // Short: superseded searches are cancelled, so typing never
            // waits on the previous query (§11).
            .search_delay(50)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label("Fuzzy Find")]);
        // Every scope is shown with its key. Like the list, the buttons
        // never take the keyboard from the search field: Tab, Shift+Tab and
        // Alt+1…3 pick a scope from there, and the window's key bindings
        // can't act on the workspace while the overlay is open.
        let scopes = gtk::Box::builder()
            .css_classes(["linked", "seshat-fuzzy-scopes"])
            .accessible_role(gtk::AccessibleRole::Group)
            .build();
        scopes.update_property(&[gtk::accessible::Property::Label("Search in")]);
        let mut scope_buttons: Vec<gtk::ToggleButton> = Vec::new();
        for choice in ScopeChoice::ALL {
            let (name, number) = (scope_name(choice), choice.number());
            let button = gtk::ToggleButton::builder()
                .label(scope_button_label(choice))
                .focusable(false)
                .can_focus(false)
                .tooltip_text(format!("{name} (Alt+{number}, or Tab to cycle)"))
                .build();
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "{name}, Alt+{number}"
            ))]);
            if let Some(first) = scope_buttons.first() {
                button.set_group(Some(first));
            }
            scopes.append(&button);
            scope_buttons.push(button);
        }
        let scope = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["seshat-fuzzy-scope", "dim-label"])
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .build();
        let scope_row = gtk::Box::builder().spacing(8).build();
        scope_row.append(&scopes);
        scope_row.append(&scope);
        let status = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["seshat-fuzzy-status", "dim-label"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::builder()
            .model(&store)
            .autoselect(false)
            .can_unselect(true)
            .build();
        let list = gtk::ListView::new(Some(selection.clone()), Some(row_factory()));
        // Keyboard focus stays in the search field; the list is driven
        // from there (§8), so a click never takes the keys away from it.
        list.set_focusable(false);
        list.set_can_focus(false);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .build();

        let hints = gtk::Label::builder()
            .label("Enter open · Alt+Enter reveal · Tab scope · Esc close")
            .xalign(1.0)
            .css_classes(["seshat-fuzzy-hints", "dim-label"])
            .build();
        let footer = gtk::Box::builder().spacing(12).build();
        footer.append(&status);
        footer.append(&hints);
        status.set_hexpand(true);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(560)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .margin_top(24)
            .css_classes(["seshat-fuzzy"])
            .visible(false)
            .build();
        root.append(&entry);
        root.append(&scope_row);
        root.append(&scroller);
        root.append(&footer);

        let updating = Rc::new(Cell::new(false));
        entry.connect_search_changed({
            let on_event = on_event.clone();
            move |entry| on_event(FuzzyViewEvent::Changed(entry.text().to_string()))
        });
        entry.connect_activate({
            let on_event = on_event.clone();
            move |_| on_event(FuzzyViewEvent::Activate)
        });
        entry.connect_stop_search({
            let on_event = on_event.clone();
            move |_| on_event(FuzzyViewEvent::Close)
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let on_event = on_event.clone();
            move |_, key, _, state| match overlay_key(key, state) {
                Some(event) => {
                    on_event(event);
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
        entry.add_controller(keys);
        for (button, choice) in scope_buttons.iter().zip(ScopeChoice::ALL) {
            button.connect_toggled({
                let on_event = on_event.clone();
                let updating = updating.clone();
                let entry = entry.clone();
                move |button| {
                    if updating.get() || !button.is_active() {
                        return;
                    }
                    on_event(FuzzyViewEvent::Scope(choice));
                    back_to_typing(&entry);
                }
            });
        }
        selection.connect_selected_notify({
            let on_event = on_event.clone();
            let updating = updating.clone();
            let entry = entry.clone();
            move |selection| {
                let position = selection.selected();
                if updating.get() || position == gtk::INVALID_LIST_POSITION {
                    return;
                }
                on_event(FuzzyViewEvent::Highlight(position));
                back_to_typing(&entry);
            }
        });
        list.connect_activate({
            let on_event = on_event.clone();
            move |_, position| {
                on_event(FuzzyViewEvent::Highlight(position));
                on_event(FuzzyViewEvent::Activate);
            }
        });

        Self {
            root,
            entry,
            scopes,
            scope_buttons,
            scope,
            status,
            store,
            selection,
            list,
            scroller,
            updating,
        }
    }

    /// Shows the overlay empty, with the keyboard in its search field.
    pub fn show(&self) {
        self.set_results(&[], None);
        self.status.set_label("");
        self.entry.set_text("");
        self.root.set_visible(true);
        self.entry.grab_focus();
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
        self.set_results(&[], None);
    }

    /// Whether `widget` belongs to the overlay (to tell when focus left).
    pub fn contains(&self, widget: &gtk::Widget) -> bool {
        widget == self.root.upcast_ref::<gtk::Widget>() || widget.is_ancestor(&self.root)
    }

    /// Shows the chosen scope and the folders searched. `chosen` is `None`
    /// while the query names a folder (F10): the scopes don't apply.
    pub fn set_scope(&self, chosen: Option<ScopeChoice>, folders: &str) {
        self.updating.set(true);
        if let Some(chosen) = chosen {
            for (button, choice) in self.scope_buttons.iter().zip(ScopeChoice::ALL) {
                button.set_active(choice == chosen);
            }
        }
        self.updating.set(false);
        self.scopes.set_sensitive(chosen.is_some());
        self.scope.set_label(folders);
        self.scope.set_tooltip_text(Some(folders));
    }

    pub fn set_status(&self, text: &str) {
        self.status.set_label(text);
    }

    /// Says `text` to screen readers, without taking the keyboard away.
    /// Announced from the status label: announcing from the focused search
    /// field hung the main loop under the UI test (GTK 4.22, 2026-11-13).
    pub fn announce(&self, text: &str) {
        self.status
            .announce(text, gtk::AccessibleAnnouncementPriority::Medium);
    }

    /// Replaces the results, highlighting `highlight`.
    pub fn set_results(&self, rows: &[ResultRow], highlight: Option<usize>) {
        self.updating.set(true);
        let rows: Vec<glib::BoxedAnyObject> = rows
            .iter()
            .cloned()
            .map(glib::BoxedAnyObject::new)
            .collect();
        self.store.splice(0, self.store.n_items(), &rows);
        self.updating.set(false);
        self.set_highlight(highlight);
    }

    /// Highlights the result at `index` and scrolls it into view.
    pub fn set_highlight(&self, index: Option<usize>) {
        self.updating.set(true);
        match index.and_then(|i| u32::try_from(i).ok()) {
            Some(position) if position < self.store.n_items() => {
                self.selection.set_selected(position);
                self.list
                    .scroll_to(position, gtk::ListScrollFlags::NONE, None);
            }
            _ => {
                self.selection.set_selected(gtk::INVALID_LIST_POSITION);
                self.scroller.vadjustment().set_value(0.0);
            }
        }
        self.updating.set(false);
    }
}

/// Gives the keyboard back to the search field after a click, with the
/// caret where it was: grabbing focus alone would select the query text.
fn back_to_typing(entry: &gtk::SearchEntry) {
    let caret = entry.position();
    entry.grab_focus();
    entry.set_position(caret);
}

/// The overlay's own keys in the search field (§8). Everything else,
/// Ctrl+Backspace included, is ordinary text editing.
fn overlay_key(key: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> Option<FuzzyViewEvent> {
    use gtk::gdk::{Key, ModifierType};
    let modifiers = state & (ModifierType::CONTROL_MASK | ModifierType::ALT_MASK);
    if modifiers == ModifierType::ALT_MASK {
        return match key {
            Key::Return | Key::KP_Enter | Key::ISO_Enter => Some(FuzzyViewEvent::Reveal),
            _ => {
                let digit = key.to_unicode()?.to_digit(10)? as usize;
                ScopeChoice::from_number(digit).map(FuzzyViewEvent::Scope)
            }
        };
    }
    if !modifiers.is_empty() {
        return None;
    }
    // Shift+Tab arrives as ISO_Left_Tab on most keymaps, as Tab with Shift
    // on others.
    let shift = state.contains(ModifierType::SHIFT_MASK);
    match key {
        Key::Tab if !shift => return Some(FuzzyViewEvent::CycleScope(1)),
        Key::Tab | Key::ISO_Left_Tab => return Some(FuzzyViewEvent::CycleScope(-1)),
        _ => {}
    }
    let step = match key {
        Key::Down => 1,
        Key::Up => -1,
        Key::Page_Down => PAGE,
        Key::Page_Up => -PAGE,
        _ => return None,
    };
    Some(FuzzyViewEvent::Move(step))
}

fn row_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().expect("list item");
        let icon = gtk::Image::new();
        let name = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let context = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .css_classes(["dim-label", "seshat-fuzzy-context"])
            .build();
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        text.append(&name);
        text.append(&context);
        let row = gtk::Box::builder()
            .spacing(8)
            .css_classes(["seshat-fuzzy-row"])
            .build();
        let badge = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .css_classes(["seshat-branch-badge"])
            .visible(false)
            .build();
        row.append(&icon);
        row.append(&text);
        row.append(&badge);
        item.set_child(Some(&row));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().expect("list item");
        let Some(object) = item.item() else { return };
        let result = object
            .downcast_ref::<glib::BoxedAnyObject>()
            .expect("results are BoxedAnyObject")
            .borrow::<ResultRow>()
            .clone();
        let row = item.child().expect("row set up");
        let mut children = std::iter::successors(row.first_child(), |w| w.next_sibling());
        let icon = children.next().and_downcast::<gtk::Image>().expect("icon");
        let text = children.next().expect("text box");
        let name = text
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("name");
        let context = name
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("context");
        let badge = text
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("badge");
        let row_text = RowText::of(&result);
        icon.set_icon_name(Some(row_text.icon));
        name.set_markup(&row_text.name_markup);
        context.set_markup(&row_text.context_markup);
        badge.set_visible(row_text.badge.is_some());
        badge.set_label(row_text.badge.as_deref().unwrap_or_default());
        row.update_property(&[gtk::accessible::Property::Label(&row_text.spoken)]);
    });
    factory
}

/// What a result row shows. Built without GTK so it can be tested.
#[derive(Debug, PartialEq, Eq)]
struct RowText {
    icon: &'static str,
    /// The name, matched characters marked, a `/` after folders: the kind
    /// is never shown by colour alone (§16).
    name_markup: String,
    /// Where it is: the folder it's in, or for a match on the path, the
    /// path with the matched characters marked.
    context_markup: String,
    /// The number of the branch it was found in, when several are searched.
    badge: Option<String>,
    /// For screen readers.
    spoken: String,
}

impl RowText {
    fn of(row: &ResultRow) -> Self {
        let result = &row.result;
        let folder = result.kind.is_browsable();
        let mut name_markup = highlighted(&result.name, &result.name_match.ranges);
        if folder {
            name_markup.push('/');
        }
        let parent = result
            .relative_path
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent);
        let context_markup = if result.path_match.is_empty() {
            match parent {
                "" => "here".to_owned(),
                parent => format!("in {}", glib::markup_escape_text(parent)),
            }
        } else {
            highlighted(&result.relative_path, &result.path_match.ranges)
        };
        let kind = if folder { "folder" } else { "file" };
        let mut spoken = match parent {
            "" => format!("{}, {kind}", result.name),
            parent => format!("{}, {kind}, in {parent}", result.name),
        };
        if let Some(branch) = row.branch {
            spoken.push_str(&format!(", branch {branch}"));
        }
        Self {
            icon: if folder {
                "folder-symbolic"
            } else {
                "text-x-generic-symbolic"
            },
            name_markup,
            context_markup,
            badge: row.branch.map(|n| n.to_string()),
            spoken,
        }
    }
}

/// `text` as Pango markup with the byte `ranges` (on character
/// boundaries, from the matcher) in bold and underlined. Everything is
/// escaped; the text itself is never changed (§5.2).
fn highlighted(text: &str, ranges: &[Range<usize>]) -> String {
    let mut markup = String::new();
    let mut at = 0;
    for range in ranges {
        let (start, end) = (range.start.min(text.len()), range.end.min(text.len()));
        if start < at || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue; // never expected: the matcher keeps to boundaries
        }
        markup.push_str(&glib::markup_escape_text(&text[at..start]));
        markup.push_str("<b><u>");
        markup.push_str(&glib::markup_escape_text(&text[start..end]));
        markup.push_str("</u></b>");
        at = end;
    }
    markup.push_str(&glib::markup_escape_text(&text[at..]));
    markup
}

#[cfg(test)]
// Lists of one matched range are what the tests mean.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use seshat_domain::entities::{EntryKind, Location};
    use seshat_domain::search::SearchMatch;

    use super::*;

    fn result(relative: &str, kind: EntryKind, name: &[Range<usize>]) -> SearchResult {
        SearchResult {
            location: Location::Path(format!("/r/{relative}").into()),
            kind,
            name: relative.rsplit('/').next().unwrap().to_owned(),
            relative_path: relative.to_owned(),
            origin: None,
            score: 1,
            name_match: SearchMatch {
                ranges: name.to_vec(),
            },
            path_match: SearchMatch::default(),
        }
    }

    fn row(result: SearchResult) -> ResultRow {
        ResultRow {
            result,
            branch: None,
        }
    }

    #[test]
    fn matches_are_marked_and_everything_is_escaped() {
        assert_eq!(
            highlighted("Documents", &[0..1, 2..3]),
            "<b><u>D</u></b>o<b><u>c</u></b>uments"
        );
        assert_eq!(
            highlighted("a<b>&c", &[0..1]),
            "<b><u>a</u></b>&lt;b&gt;&amp;c"
        );
        assert_eq!(highlighted("plain", &[]), "plain");
    }

    #[test]
    fn highlights_keep_whole_characters() {
        // "é" as e + combining accent: the matcher's range covers both.
        let name = "cafe\u{301}";
        assert_eq!(highlighted(name, &[3..6]), "caf<b><u>e\u{301}</u></b>");
        // A range not on a character boundary is skipped, never split.
        assert_eq!(highlighted("é", &[0..1]), "é");
    }

    #[test]
    fn folders_and_files_differ_by_more_than_colour() {
        let folder = RowText::of(&row(result(
            "Development/Code",
            EntryKind::Directory,
            &[0..1],
        )));
        assert_eq!(folder.icon, "folder-symbolic");
        assert_eq!(folder.name_markup, "<b><u>C</u></b>ode/");
        assert_eq!(folder.context_markup, "in Development");
        assert_eq!(folder.spoken, "Code, folder, in Development");

        let file = RowText::of(&row(result("notes.txt", EntryKind::File, &[])));
        assert_eq!(file.icon, "text-x-generic-symbolic");
        assert_eq!(file.name_markup, "notes.txt");
        assert_eq!(file.context_markup, "here");
        assert_eq!(file.spoken, "notes.txt, file");

        let link = RowText::of(&row(result(
            "l",
            EntryKind::Symlink { to_directory: true },
            &[],
        )));
        assert_eq!(link.name_markup, "l/", "a link to a folder opens as one");
    }

    #[test]
    fn a_match_on_the_path_shows_the_path_marked() {
        let mut r = result("Development/Code/Modules", EntryKind::Directory, &[]);
        r.path_match = SearchMatch {
            ranges: vec![0..1, 12..13],
        };
        assert_eq!(
            RowText::of(&row(r)).context_markup,
            "<b><u>D</u></b>evelopment/<b><u>C</u></b>ode/Modules"
        );
    }

    #[test]
    fn results_from_several_branches_carry_the_branch_number() {
        let mut found = row(result("Camera", EntryKind::Directory, &[]));
        found.branch = Some(2);
        let text = RowText::of(&found);
        assert_eq!(text.badge.as_deref(), Some("2"));
        assert_eq!(text.spoken, "Camera, folder, branch 2");
        assert_eq!(RowText::of(&row(found.result)).badge, None);
    }

    #[test]
    fn scope_buttons_show_their_alt_number() {
        let labels: Vec<String> = ScopeChoice::ALL
            .into_iter()
            .map(scope_button_label)
            .collect();
        assert_eq!(labels, ["1 Directory", "2 Branch", "3 Workspace"]);
    }

    #[test]
    fn the_overlay_keys_leave_text_editing_alone() {
        use gtk::gdk::{Key, ModifierType as M};
        let none = M::empty();
        assert_eq!(overlay_key(Key::Down, none), Some(FuzzyViewEvent::Move(1)));
        assert_eq!(
            overlay_key(Key::Page_Up, none),
            Some(FuzzyViewEvent::Move(-PAGE))
        );
        assert_eq!(
            overlay_key(Key::Return, M::ALT_MASK),
            Some(FuzzyViewEvent::Reveal)
        );
        assert_eq!(
            overlay_key(Key::_3, M::ALT_MASK),
            Some(FuzzyViewEvent::Scope(ScopeChoice::EntireWorkspace))
        );
        assert_eq!(overlay_key(Key::_4, M::ALT_MASK), None);
        assert_eq!(overlay_key(Key::_0, M::ALT_MASK), None);
        assert_eq!(
            overlay_key(Key::Return, none),
            None,
            "Enter is the entry's own"
        );
        assert_eq!(overlay_key(Key::BackSpace, M::CONTROL_MASK), None);
        assert_eq!(overlay_key(Key::Down, M::CONTROL_MASK), None);
        assert_eq!(overlay_key(Key::a, none), None);
        assert_eq!(
            overlay_key(Key::Tab, none),
            Some(FuzzyViewEvent::CycleScope(1))
        );
        assert_eq!(
            overlay_key(Key::ISO_Left_Tab, M::SHIFT_MASK),
            Some(FuzzyViewEvent::CycleScope(-1))
        );
        assert_eq!(
            overlay_key(Key::Tab, M::SHIFT_MASK),
            Some(FuzzyViewEvent::CycleScope(-1))
        );
        assert_eq!(
            overlay_key(Key::Tab, M::CONTROL_MASK),
            None,
            "Ctrl+Tab is GTK's"
        );
    }
}
