//! What the shell reports when a command cannot be carried out.
//!
//! The Bash script printed `wayfind: <message>` on standard error and exited 1,
//! whatever went wrong. Scripts and agents read that line, so this keeps the
//! same shape. Every error the shell can produce reduces to one line of prose,
//! and the variant only records where it came from.

use std::path::PathBuf;

use thiserror::Error;

/// Everything the shell can refuse to do.
#[derive(Debug, Error)]
pub enum ShellError {
    /// A configuration file could not be read.
    #[error("cannot read {path}: {source}")]
    ConfigUnreadable {
        /// The file that was asked for.
        path: PathBuf,
        /// Why the read failed.
        source: std::io::Error,
    },

    /// Neither `$XDG_CONFIG_HOME` nor `$HOME` was set, so there is no default
    /// place to look and no layer said where to look instead.
    #[error(
        "cannot find a configuration directory; set XDG_CONFIG_HOME or HOME, \
         or pass --sqlite.database"
    )]
    NoConfigHome,

    /// A value did not survive the core's checks.
    #[error(transparent)]
    Domain(#[from] wayfind_v1_core::Error),

    /// The store could not answer.
    #[error(transparent)]
    Storage(#[from] wayfind_v1_core::StorageError),

    /// The search index could not answer.
    #[error(transparent)]
    Search(#[from] wayfind_v1_core::SearchError),

    /// A file the operator named could not be used.
    ///
    /// The path is carried separately because `std::io::Error` never holds one,
    /// and "no such file" without a name is no help to anybody.
    #[error("cannot {action} {path}: {source}")]
    File {
        /// What was being attempted.
        action: &'static str,
        /// The file it was attempted on.
        path: PathBuf,
        /// Why it failed.
        source: std::io::Error,
    },

    /// Standard input or standard output failed.
    #[error("cannot {action}: {source}")]
    Stream {
        /// What was being attempted.
        action: &'static str,
        /// Why it failed.
        source: std::io::Error,
    },

    /// The command was well formed and the data says no.
    #[error("{0}")]
    Refused(String),
}

impl ShellError {
    /// Refuse a command with a plain sentence.
    pub fn refused(message: impl Into<String>) -> Self {
        ShellError::Refused(message.into())
    }

    /// Report a failure against a named file.
    pub fn file(action: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ShellError::File {
            action,
            path: path.into(),
            source,
        }
    }

    /// Report a failure against a stream that has no name.
    pub fn stream(action: &'static str, source: std::io::Error) -> Self {
        ShellError::Stream { action, source }
    }
}

/// What a shell operation returns.
pub type ShellResult<T> = std::result::Result<T, ShellError>;
