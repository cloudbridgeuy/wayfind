//! The SQLite adapter.
//!
//! One file holds everything: the records, the attachment bytes, and the
//! full-text index. That is the shape the Bash script chose, and keeping it is
//! the whole point of the port — an operator upgrades by replacing one program,
//! not by moving their data.
//!
//! Nothing here decides anything. The adapter opens a connection, turns rows
//! into the core's values, and turns the core's values back into rows. Every
//! rule about what those values *mean* lives in `wayfind_core`.

pub mod row;
pub mod schema;

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use wayfind_core::{StorageError, StorageResult};

/// An open connection to a wayfind database.
#[derive(Debug)]
pub struct SqliteStorage {
    connection: Connection,
}

impl SqliteStorage {
    /// Open a database that already exists.
    ///
    /// The file is opened exactly as it is found. The journal mode is left
    /// alone: an operator who chose one deliberately keeps it, and a database
    /// the script created is already in write-ahead logging. No table is
    /// created either, so a command aimed at the wrong path says so instead of
    /// quietly leaving an empty database behind.
    ///
    /// The one change made to an existing file is the additive `amended_at`
    /// column, which is what the script itself does on every start.
    pub fn open(path: &Path) -> StorageResult<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI;
        let connection = Connection::open_with_flags(path, flags).map_err(|error| {
            StorageError::infrastructure(
                "open database",
                format!(
                    "cannot open {}: {error}; run `wayfind init` to create it",
                    path.display()
                ),
            )
        })?;
        let storage = Self { connection };
        storage.enforce_foreign_keys()?;
        schema::ensure_amended_at(&storage.connection)?;
        Ok(storage)
    }

    /// Create the database if it is absent, then open it.
    ///
    /// Only `wayfind init` calls this. It is the one place allowed to make a
    /// directory or a file, and the one place that sets the journal mode.
    pub fn initialize(path: &Path) -> StorageResult<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                StorageError::infrastructure(
                    "create data directory",
                    format!("cannot create {}: {error}", parent.display()),
                )
            })?;
        }
        let connection = Connection::open(path).map_err(|error| {
            StorageError::infrastructure(
                "create database",
                format!("cannot open {}: {error}", path.display()),
            )
        })?;
        let storage = Self { connection };
        storage.enforce_foreign_keys()?;
        schema::enable_write_ahead_logging(&storage.connection)?;
        schema::create_schema(&storage.connection)?;
        schema::ensure_amended_at(&storage.connection)?;
        Ok(storage)
    }

    /// The open connection, for the capabilities built on top of it.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Turn on referential integrity for this connection.
    ///
    /// SQLite defaults this off for backwards compatibility, and it is a
    /// per-connection setting rather than a stored one. Every connection has to
    /// ask for it, exactly as each of the script's `sqlite3` invocations did.
    fn enforce_foreign_keys(&self) -> StorageResult<()> {
        self.connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| StorageError::infrastructure("enable foreign keys", error.to_string()))
    }
}
