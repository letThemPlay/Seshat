//! The address bar: breadcrumbs, or a text field to type a location into
//! (Ctrl+L). Typed text is resolved against Home and the active panel's
//! folder; a location from outside the tree opens as decision D3 says.
//! The breadcrumbs themselves are drawn by the window's `update_view`
//! from [`AddressBarController::editing`].

use std::path::PathBuf;

use seshat_application::navigation::{AddressError, parse_address};
use seshat_application::workspace::WorkspaceService;
use seshat_domain::entities::Location;

use super::Cx;
use crate::notice::Notice;

#[derive(Debug)]
pub enum AddressMsg {
    /// Escape in the text field.
    Cancel,
    /// Return in the text field.
    Submitted(String),
}

pub(super) struct AddressBarController {
    home: PathBuf,
    editing: bool,
}

impl AddressBarController {
    pub(super) fn new(home: PathBuf) -> Self {
        Self {
            home,
            editing: false,
        }
    }

    /// Switches from breadcrumbs to the text field (Ctrl+L, the edit button).
    pub(super) fn edit(&mut self) {
        self.editing = true;
    }

    /// Whether the text field is shown instead of the breadcrumbs.
    pub(super) fn editing(&self) -> bool {
        self.editing
    }

    pub(super) fn update(&mut self, msg: AddressMsg, cx: &mut Cx<'_>) {
        match msg {
            AddressMsg::Cancel => self.editing = false,
            AddressMsg::Submitted(text) => self.submit(&text, cx.service, cx.notice),
        }
    }

    /// Opens the typed location (decision D3), or says why it can't and
    /// keeps the text field open for a correction.
    fn submit(&mut self, text: &str, service: &mut WorkspaceService, notice: &mut Option<Notice>) {
        match self.resolve(text, service.active_location()) {
            Ok(location) => {
                self.editing = false;
                service.open_location(location);
            }
            Err(err) => *notice = Some(Notice::from_error(&err)),
        }
    }

    /// Goes back to the breadcrumbs. Returns whether the field was open.
    pub(super) fn stop_editing(&mut self) -> bool {
        std::mem::replace(&mut self.editing, false)
    }

    /// Text from the address bar, `:open`, `:copy` or `:move`, resolved
    /// against Home and `current`, the active panel's folder.
    pub(super) fn resolve(&self, text: &str, current: &Location) -> Result<Location, AddressError> {
        parse_address(text, &self.home, Some(current))
    }
}

#[cfg(test)]
mod tests {
    use seshat_domain::workspace::PanelState;

    use super::*;

    fn service() -> WorkspaceService {
        WorkspaceService::starting_at(Location::Path("/home/u".into()), PanelState::default())
    }

    #[test]
    fn a_typed_location_opens_and_closes_the_field() {
        let mut address = AddressBarController::new("/home/u".into());
        let mut service = service();
        let mut notice = None;
        address.edit();
        address.submit("~/Documents", &mut service, &mut notice);
        assert!(!address.editing());
        assert_eq!(notice, None);
        assert_eq!(
            service.active_location(),
            &Location::Path("/home/u/Documents".into())
        );
    }

    #[test]
    fn an_invalid_location_keeps_the_field_open_and_says_why() {
        let mut address = AddressBarController::new("/home/u".into());
        let mut service = service();
        let mut notice = None;
        address.edit();
        address.submit("   ", &mut service, &mut notice);
        assert!(address.editing(), "the user can correct it");
        assert_eq!(
            notice.map(|n| n.text().to_owned()).as_deref(),
            Some("Enter a location.")
        );
        assert_eq!(service.active_location(), &Location::Path("/home/u".into()));
    }

    #[test]
    fn stop_editing_reports_whether_the_field_was_open() {
        let mut address = AddressBarController::new("/home/u".into());
        assert!(!address.stop_editing());
        address.edit();
        assert!(address.stop_editing());
        assert!(!address.editing());
    }

    #[test]
    fn text_is_resolved_against_home_and_the_active_folder() {
        let address = AddressBarController::new("/home/u".into());
        let current = Location::Path("/srv/data".into());
        assert_eq!(
            address.resolve("~", &current),
            Ok(Location::Path("/home/u".into()))
        );
        assert_eq!(
            address.resolve("logs", &current),
            Ok(Location::Path("/srv/data/logs".into()))
        );
    }
}
