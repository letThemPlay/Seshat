//! Moving through an ordered list, one step at a time.

/// Which way to step through a list: branches (`]`, `[`) or command-line
/// matches (Tab, Shift+Tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Next,
    Previous,
}

impl Direction {
    /// The index one step from `index` in a list of `len` items, wrapping
    /// around at both ends. From no index, `Next` starts at the first item
    /// and `Previous` at the last. `None` for an empty list.
    pub fn wrapping_step(self, index: Option<usize>, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        Some(match (index, self) {
            (None, Self::Next) => 0,
            (None, Self::Previous) => len - 1,
            (Some(i), Self::Next) => (i + 1) % len,
            (Some(i), Self::Previous) => (i + len - 1) % len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Direction::{Next, Previous};

    #[test]
    fn steps_wrap_around_and_start_at_the_matching_end() {
        assert_eq!(Next.wrapping_step(None, 3), Some(0));
        assert_eq!(Previous.wrapping_step(None, 3), Some(2));
        assert_eq!(Next.wrapping_step(Some(1), 3), Some(2));
        assert_eq!(Next.wrapping_step(Some(2), 3), Some(0));
        assert_eq!(Previous.wrapping_step(Some(0), 3), Some(2));
        assert_eq!(Previous.wrapping_step(Some(2), 3), Some(1));
        assert_eq!(Next.wrapping_step(None, 0), None);
        assert_eq!(Previous.wrapping_step(Some(0), 0), None);
    }
}
