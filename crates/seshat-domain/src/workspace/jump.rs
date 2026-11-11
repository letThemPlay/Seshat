//! The workspace jump list: back/forward through panel activations
//! (decision D2).

use std::collections::{HashSet, VecDeque};

use super::PanelId;

/// Back/forward history of panel activations. Bounded, and it never
/// creates or destroys panels: it only remembers which ones were active.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JumpList {
    back: VecDeque<PanelId>,
    forward: Vec<PanelId>,
}

impl JumpList {
    /// Recorded activations kept at most; the oldest is dropped first.
    pub const LIMIT: usize = 100;

    /// Rebuilds a jump list (persistence), oldest entry first.
    pub fn from_parts(back: Vec<PanelId>, forward: Vec<PanelId>) -> Self {
        Self {
            back: back.into(),
            forward,
        }
    }

    /// The back history, oldest first.
    pub fn back(&self) -> Vec<PanelId> {
        self.back.iter().copied().collect()
    }

    /// The forward history; the next step forward is last.
    pub fn forward(&self) -> Vec<PanelId> {
        self.forward.clone()
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// `previous` was left by an activation worth remembering: it becomes
    /// the step back, and the forward history no longer applies.
    pub fn record(&mut self, previous: PanelId) {
        self.back.push_back(previous);
        if self.back.len() > Self::LIMIT {
            self.back.pop_front();
        }
        self.forward.clear();
    }

    /// The next panel back (or forward) that still `exists` and isn't
    /// `current`, skipping stale entries. `current` moves to the other
    /// side, so the step can be retraced. `None` when there is nowhere to
    /// go.
    pub fn step(
        &mut self,
        back: bool,
        current: PanelId,
        exists: impl Fn(PanelId) -> bool,
    ) -> Option<PanelId> {
        loop {
            let target = if back {
                self.back.pop_back()
            } else {
                self.forward.pop()
            }?;
            if !exists(target) || target == current {
                continue;
            }
            if back {
                self.forward.push(current);
            } else {
                self.back.push_back(current);
            }
            return Some(target);
        }
    }

    /// Panels removed from the workspace leave the history; repeats left
    /// next to each other collapse into one.
    pub fn forget(&mut self, gone: &HashSet<PanelId>) {
        self.back.retain(|p| !gone.contains(p));
        self.forward.retain(|p| !gone.contains(p));
        let mut back: Vec<PanelId> = self.back.drain(..).collect();
        back.dedup();
        self.back = back.into();
        self.forward.dedup();
    }

    /// Every panel the history refers to.
    pub fn panels(&self) -> impl Iterator<Item = PanelId> + '_ {
        self.back.iter().chain(&self.forward).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(raw: u64) -> PanelId {
        PanelId::from_raw(raw)
    }

    #[test]
    fn records_are_bounded_dropping_the_oldest() {
        let mut jumps = JumpList::default();
        for raw in 0..(JumpList::LIMIT as u64 + 5) {
            jumps.record(p(raw));
        }
        let back = jumps.back();
        assert_eq!(back.len(), JumpList::LIMIT);
        assert_eq!(back.first(), Some(&p(5)), "the oldest five were dropped");
        assert_eq!(back.last(), Some(&p(JumpList::LIMIT as u64 + 4)));
    }

    #[test]
    fn a_new_record_drops_the_forward_history() {
        let mut jumps = JumpList::from_parts(vec![p(1)], vec![p(3)]);
        assert!(jumps.can_go_forward());
        jumps.record(p(2));
        assert!(!jumps.can_go_forward());
        assert_eq!(jumps.back(), [p(1), p(2)]);
    }

    #[test]
    fn steps_skip_missing_and_current_panels_and_can_be_retraced() {
        // Back history 1, 2 (gone), 9 (current); we are at 9.
        let mut jumps = JumpList::from_parts(vec![p(1), p(2), p(9)], Vec::new());
        let exists = |panel: PanelId| panel != p(2);
        assert_eq!(jumps.step(true, p(9), exists), Some(p(1)));
        assert!(!jumps.can_go_back());
        assert_eq!(jumps.forward(), [p(9)]);
        // Forward again returns to 9, and 1 goes back on the back side.
        assert_eq!(jumps.step(false, p(1), exists), Some(p(9)));
        assert_eq!(jumps.back(), [p(1)]);
        // Nowhere left to go forward.
        assert_eq!(jumps.step(false, p(9), exists), None);
    }

    #[test]
    fn forgetting_panels_collapses_repeats() {
        let mut jumps = JumpList::from_parts(vec![p(1), p(2), p(1), p(3)], vec![p(4), p(5), p(4)]);
        jumps.forget(&HashSet::from([p(2), p(5)]));
        assert_eq!(jumps.back(), [p(1), p(3)]);
        assert_eq!(jumps.forward(), [p(4)]);
        let all: Vec<PanelId> = jumps.panels().collect();
        assert_eq!(all, [p(1), p(3), p(4)]);
    }

    #[test]
    fn parts_round_trip() {
        let jumps = JumpList::from_parts(vec![p(1), p(2)], vec![p(3)]);
        assert_eq!(JumpList::from_parts(jumps.back(), jumps.forward()), jumps);
    }
}
