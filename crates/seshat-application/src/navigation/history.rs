use seshat_domain::entities::Location;

/// Back/forward navigation history.
#[derive(Debug, Clone)]
pub struct NavigationHistory {
    back: Vec<Location>,
    current: Option<Location>,
    forward: Vec<Location>,
    limit: usize,
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self::with_limit(100)
    }
}

impl NavigationHistory {
    /// `limit` bounds the back stack so history cannot grow without bound.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            back: Vec::new(),
            current: None,
            forward: Vec::new(),
            limit: limit.max(1),
        }
    }

    pub fn current(&self) -> Option<&Location> {
        self.current.as_ref()
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Records a visit to `location`. Re-visiting the current location is a
    /// no-op so refreshes don't pollute history. Returns whether it changed.
    pub fn visit(&mut self, location: Location) -> bool {
        if self.current.as_ref() == Some(&location) {
            return false;
        }
        if let Some(previous) = self.current.replace(location) {
            self.back.push(previous);
            if self.back.len() > self.limit {
                self.back.remove(0);
            }
        }
        self.forward.clear();
        true
    }

    /// Replaces the current location without recording history, e.g. when
    /// the current location disappeared and we fell back elsewhere.
    pub fn replace_current(&mut self, location: Location) {
        self.current = Some(location);
    }

    pub fn go_back(&mut self) -> Option<&Location> {
        let target = self.back.pop()?;
        if let Some(current) = self.current.replace(target) {
            self.forward.push(current);
        }
        self.current.as_ref()
    }

    pub fn go_forward(&mut self) -> Option<&Location> {
        let target = self.forward.pop()?;
        if let Some(current) = self.current.replace(target) {
            self.back.push(current);
        }
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Location {
        Location::Path(s.into())
    }

    #[test]
    fn back_and_forward() {
        let mut h = NavigationHistory::default();
        h.visit(p("/a"));
        h.visit(p("/b"));
        h.visit(p("/c"));
        assert_eq!(h.go_back(), Some(&p("/b")));
        assert_eq!(h.go_back(), Some(&p("/a")));
        assert_eq!(h.go_back(), None);
        assert_eq!(h.current(), Some(&p("/a")));
        assert_eq!(h.go_forward(), Some(&p("/b")));
        assert!(h.can_go_forward());
        assert!(h.can_go_back());
    }

    #[test]
    fn visiting_clears_forward() {
        let mut h = NavigationHistory::default();
        h.visit(p("/a"));
        h.visit(p("/b"));
        h.go_back();
        h.visit(p("/x"));
        assert!(!h.can_go_forward());
        assert_eq!(h.go_back(), Some(&p("/a")));
    }

    #[test]
    fn revisiting_current_is_a_noop() {
        let mut h = NavigationHistory::default();
        assert!(h.visit(p("/a")));
        assert!(!h.visit(p("/a")));
        assert!(!h.can_go_back());
    }

    #[test]
    fn back_stack_is_bounded() {
        let mut h = NavigationHistory::with_limit(2);
        for s in ["/1", "/2", "/3", "/4"] {
            h.visit(p(s));
        }
        assert_eq!(h.go_back(), Some(&p("/3")));
        assert_eq!(h.go_back(), Some(&p("/2")));
        assert_eq!(h.go_back(), None);
    }

    #[test]
    fn replace_current_does_not_record_history() {
        let mut h = NavigationHistory::default();
        h.visit(p("/a"));
        h.visit(p("/usb"));
        h.replace_current(p("/home"));
        assert_eq!(h.current(), Some(&p("/home")));
        assert_eq!(h.go_back(), Some(&p("/a")));
    }
}
