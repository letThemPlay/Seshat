//! Navigation state and helpers. Pure logic, independent of the UI toolkit.

mod address;
mod breadcrumbs;

pub use address::{AddressError, parse_address};
pub use breadcrumbs::{Breadcrumb, breadcrumbs};
