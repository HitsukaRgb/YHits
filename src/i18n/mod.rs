#[macro_use]
mod macros;

mod catalog;
mod detect;
mod format;
mod language;

pub use catalog::{t, Tr};
pub use detect::detect_system_language;
pub use format::tf;
pub use language::{Language, current_language, set_language};
