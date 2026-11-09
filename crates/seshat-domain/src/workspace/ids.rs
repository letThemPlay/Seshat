use std::fmt;

/// Stable identifier of a panel within a workspace. Never reused, and kept
/// across restarts by workspace persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PanelId(u64);

/// Stable identifier of a branch within a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchId(u64);

macro_rules! raw_id {
    ($ty:ident, $prefix:literal) => {
        impl $ty {
            /// Rebuilds an id from its raw value (persistence only).
            pub fn from_raw(raw: u64) -> Self {
                Self(raw)
            }

            pub fn raw(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

raw_id!(PanelId, "p");
raw_id!(BranchId, "b");
