//! Validation: turning a command into a value ready to write verbatim.
//!
//! Every prepared value in here is constructible only by the module that
//! validates it — an unvalidated write must be unspellable.

pub mod initiative;
