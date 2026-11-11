//! Fuzzy Find adapters: the fuzzy matcher and the folder walker behind the
//! application's search ports.

mod nucleo;
mod walker;

pub use nucleo::NucleoMatcher;
pub use walker::LocalTreeWalker;
