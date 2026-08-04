//! Every document Wayfind prints, rendered from values.
//!
//! A render function is total: it takes a value and returns a `String`. It
//! reads no clock, opens no file, and cannot fail. That is what makes the
//! output contract testable without a store behind it.

pub mod error;
pub mod format;
pub mod front_matter;
pub mod initiative;

pub use front_matter::{Field, FrontMatter};
