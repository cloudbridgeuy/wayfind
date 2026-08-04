//! The handlers, one per command.
//!
//! A handler is the thin part: it collects what the command needs from the
//! shell, asks `wayfind_core` for the decision, and writes the answer. Nothing
//! here decides anything.
//!
//! Most handlers refuse in this slice. The slice creates a store and nothing
//! else, so every command that would read or write a record answers with the
//! usage token instead. The argument surface is settled, so a later slice
//! replaces a body and never a spelling.

use wayfind_core::error::{ErrorToken, Rejection};

use crate::error::ShellError;

/// The answer a command that this slice does not carry out yet gives.
///
/// It is a usage refusal, not an internal fault: the operator asked for
/// something the program will do and does not do yet, and exit 2 says exactly
/// that.
pub fn not_implemented() -> ShellError {
    ShellError::Rejected(Rejection::new(ErrorToken::Usage).body("not implemented in this slice"))
}
