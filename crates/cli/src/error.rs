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
    Domain(#[from] wayfind_core::Error),

    /// The command was well formed and the data says no.
    #[error("{0}")]
    Refused(String),
}

impl ShellError {
    /// Refuse a command with a plain sentence.
    pub fn refused(message: impl Into<String>) -> Self {
        ShellError::Refused(message.into())
    }
}

/// What a shell operation returns.
pub type ShellResult<T> = std::result::Result<T, ShellError>;
