//! The handlers, one per command.
//!
//! A handler is the thin part: it reads what it needs from the [`Shell`], asks
//! `wayfind_core` for the decision, and writes the answer through an
//! [`crate::output::Output`]. Nothing here decides anything.
//!
//! Most handlers refuse in this slice. The slice creates a store and nothing
//! else, so every command that would read or write a record answers with the
//! usage token instead. The argument surface is settled, so a later slice
//! replaces a body and never a spelling.

pub mod graph;
pub mod init;
pub mod initiative;
pub mod retired;
pub mod snapshot;

use std::path::PathBuf;

use wayfind_core::{
    error::{ErrorToken, Rejection},
    id::ProjectKey,
    storage::{coordination::CoordinationStorage, graph::GraphStorage},
    time::Timestamp,
};

use crate::{context::Environment, error::ShellError, sqlite::SqliteStore};

/// Everything a handler may need, gathered once by the composition root.
///
/// One clock reading is taken before the command runs and carried here, so
/// that everything a single command writes carries a single time.
pub struct Shell<'a> {
    /// The store, open for this command.
    pub store: &'a SqliteStore,
    /// The immutable graph: initiatives, snapshots, and their records.
    pub graph: &'a dyn GraphStorage,
    /// The coordination domain's identifier allocator.
    pub coordination: &'a dyn CoordinationStorage,
    /// The machine the program is running on.
    pub environment: &'a dyn Environment,
    /// The database file this command opened or created.
    pub database: PathBuf,
    /// The tree the operator is working in.
    pub project: ProjectKey,
    /// A session named on the command line, if one was.
    pub chosen_session: Option<String>,
    /// An initiative named on the command line, if one was.
    pub chosen_initiative: Option<i64>,
    /// One clock reading, taken before the command ran.
    pub now: Timestamp,
}

/// The answer a command that this slice does not carry out yet gives.
///
/// It is a usage refusal, not an internal fault: the operator asked for
/// something the program will do and does not do yet, and exit 2 says exactly
/// that.
pub fn not_implemented() -> ShellError {
    ShellError::Rejected(Rejection::new(ErrorToken::Usage).body("not implemented in this slice"))
}
