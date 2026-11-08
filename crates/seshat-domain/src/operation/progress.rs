use crate::DomainError;

/// Progress of a unit of work (files or bytes).
///
/// When the total is unknown the progress is indeterminate and no percentage
/// is reported, so we never show a misleading number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Indeterminate { completed: u64 },
    Determinate { completed: u64, total: u64 },
}

impl Progress {
    pub fn indeterminate(completed: u64) -> Self {
        Self::Indeterminate { completed }
    }

    pub fn determinate(completed: u64, total: u64) -> Result<Self, DomainError> {
        if completed > total {
            return Err(DomainError::ProgressExceedsTotal { completed, total });
        }
        Ok(Self::Determinate { completed, total })
    }

    pub fn completed(&self) -> u64 {
        match *self {
            Self::Indeterminate { completed } | Self::Determinate { completed, .. } => completed,
        }
    }

    pub fn total(&self) -> Option<u64> {
        match *self {
            Self::Indeterminate { .. } => None,
            Self::Determinate { total, .. } => Some(total),
        }
    }

    /// Percentage in `0.0..=100.0`, or `None` when indeterminate.
    /// An empty determinate job (total 0) is considered complete.
    pub fn percentage(&self) -> Option<f64> {
        match *self {
            Self::Indeterminate { .. } => None,
            Self::Determinate { total: 0, .. } => Some(100.0),
            Self::Determinate { completed, total } => Some(completed as f64 / total as f64 * 100.0),
        }
    }

    /// Records more completed work. Saturates at the total for determinate
    /// progress instead of overshooting.
    pub fn advance(self, amount: u64) -> Self {
        match self {
            Self::Indeterminate { completed } => Self::Indeterminate {
                completed: completed.saturating_add(amount),
            },
            Self::Determinate { completed, total } => Self::Determinate {
                completed: completed.saturating_add(amount).min(total),
                total,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indeterminate_has_no_percentage() {
        let p = Progress::indeterminate(42);
        assert_eq!(p.percentage(), None);
        assert_eq!(p.total(), None);
        assert_eq!(p.completed(), 42);
    }

    #[test]
    fn determinate_percentage() {
        let p = Progress::determinate(39_321_600, 104_857_600).unwrap();
        assert_eq!(p.percentage(), Some(37.5));
    }

    #[test]
    fn empty_job_is_complete() {
        assert_eq!(
            Progress::determinate(0, 0).unwrap().percentage(),
            Some(100.0)
        );
    }

    #[test]
    fn completed_cannot_exceed_total() {
        assert_eq!(
            Progress::determinate(11, 10),
            Err(DomainError::ProgressExceedsTotal {
                completed: 11,
                total: 10
            })
        );
    }

    #[test]
    fn advance_saturates_at_total() {
        let p = Progress::determinate(8, 10).unwrap().advance(5);
        assert_eq!(
            p,
            Progress::Determinate {
                completed: 10,
                total: 10
            }
        );
    }

    #[test]
    fn advance_indeterminate() {
        assert_eq!(Progress::indeterminate(1).advance(2).completed(), 3);
    }
}
