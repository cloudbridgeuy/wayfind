//! What the shell reports when a command cannot be carried out.
//!
//! v2 answers a refused command with an error document, not a bare line: the
//! caller reads a token and an exit code. Everything the shell can go wrong at
//! therefore reduces to a [`Rejection`] or to a fault it could not have
//! predicted, and the variant records only which of the two it was.

use std::path::PathBuf;

use thiserror::Error;
use wayfind_core::error::Rejection;

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

    /// The command was well formed and the domain says no.
    ///
    /// This is the one variant with a contract: it carries the token the caller
    /// matches on and the exit code the command ends with.
    #[error("{}", .0.token().as_token())]
    Rejected(Rejection),

    /// The store could not answer.
    #[error(transparent)]
    Storage(#[from] rusqlite::Error),

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
}

impl ShellError {
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

    /// The rejection this error prints, when it has one.
    ///
    /// Anything else is an internal fault: exit code 1, no token, because it is
    /// not a refusal the operator can answer.
    pub fn rejection(&self) -> Option<&Rejection> {
        match self {
            ShellError::Rejected(rejection) => Some(rejection),
            _ => None,
        }
    }
}

impl From<Rejection> for ShellError {
    fn from(rejection: Rejection) -> Self {
        ShellError::Rejected(rejection)
    }
}

/// What a shell operation returns.
pub type ShellResult<T> = std::result::Result<T, ShellError>;
