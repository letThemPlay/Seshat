use std::fmt;

use uuid::Uuid;

/// Unique identifier of a file operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(Uuid);

impl OperationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for OperationId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        assert_ne!(OperationId::new(), OperationId::new());
    }

    #[test]
    fn display_is_hyphenated_uuid() {
        let uuid = Uuid::nil();
        assert_eq!(
            OperationId::from(uuid).to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
    }
}
