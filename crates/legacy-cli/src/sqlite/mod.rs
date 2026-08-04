//! The SQLite adapter.
//!
//! One file holds everything: the records, the attachment bytes, and the
//! full-text index. That is the shape the Bash script chose, and keeping it is
//! the whole point of the port — an operator upgrades by replacing one program,
//! not by moving their data.
//!
//! Nothing here decides anything. The adapter opens a connection, turns rows
//! into the core's values, and turns the core's values back into rows. Every
//! rule about what those values *mean* lives in `wayfind_v1_core`.

pub mod atomic;
pub mod attachment;
pub mod read;
pub mod row;
pub mod schema;
pub mod search;
pub mod write;

use std::path::Path;

use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};
use wayfind_v1_core::{InitiativeId, InitiativeRevision, StaleRevision, StorageError, StorageResult};

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
        schema::create_adapter_schema(&storage.connection)?;
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
        schema::create_adapter_schema(&storage.connection)?;
        Ok(storage)
    }

    /// The open connection, for the capabilities built on top of it.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Start a transaction that takes its write lock immediately.
    ///
    /// Deferred transactions take a read lock first and upgrade on the first
    /// write, and two of those upgrading at once is the classic way to earn
    /// `SQLITE_BUSY` with no chance of retry. Every workflow here writes, so
    /// taking the lock up front costs nothing and removes the failure.
    pub(crate) fn write_transaction(&self) -> StorageResult<Transaction<'_>> {
        Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(failed("begin transaction"))
    }

    /// Turn on referential integrity for this connection.
    ///
    /// SQLite defaults this off for backwards compatibility, and it is a
    /// per-connection setting rather than a stored one. Every connection has to
    /// ask for it, exactly as each of the script's `sqlite3` invocations did.
    fn enforce_foreign_keys(&self) -> StorageResult<()> {
        self.connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(failed("enable foreign keys"))
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Turn a SQLite failure into an infrastructure error naming the operation.
pub(crate) fn failed(operation: &'static str) -> impl Fn(rusqlite::Error) -> StorageError {
    move |error| StorageError::infrastructure(operation, error.to_string())
}

/// The revision an initiative currently sits at.
///
/// An initiative with no counter row has never been written by this adapter, so
/// it reads as the initial revision. That is what makes a database the script
/// wrote usable without a migration: the first optimistic write against it
/// compares against zero, and the row appears when that write commits.
pub(crate) fn read_revision(
    connection: &Connection,
    initiative_id: InitiativeId,
) -> StorageResult<InitiativeRevision> {
    let stored: Option<i64> = connection
        .query_row(
            "SELECT revision FROM wayfind_initiative_revisions WHERE initiative_id = ?1;",
            [initiative_id.get()],
            |row| row.get(0),
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(failed("read initiative revision"))?;
    let value = stored.unwrap_or(0);
    let value = u64::try_from(value).map_err(|_| {
        StorageError::CorruptData(wayfind_v1_core::Error::corrupt_data(
            "initiative revision",
            format!("a revision cannot be negative, but this one is {value}"),
        ))
    })?;
    Ok(InitiativeRevision::new(value))
}

/// Compare an initiative's revision against what a command expected.
///
/// Answers `Some(stale)` when the world moved, which every caller turns into its
/// own conflict variant.
pub(crate) fn revision_conflict(
    connection: &Connection,
    initiative_id: InitiativeId,
    expected: InitiativeRevision,
) -> StorageResult<Option<StaleRevision>> {
    let actual = read_revision(connection, initiative_id)?;
    if actual == expected {
        return Ok(None);
    }
    Ok(Some(StaleRevision { expected, actual }))
}

/// Move an initiative's revision on by one, creating the counter if needed.
///
/// Called inside the same transaction as the write it describes, so a reader
/// either sees the old records at the old revision or the new records at the new
/// one, and never a mixture.
pub(crate) fn advance_revision(
    connection: &Connection,
    initiative_id: InitiativeId,
) -> StorageResult<InitiativeRevision> {
    let next = read_revision(connection, initiative_id)?.next();
    let raw = i64::try_from(next.get()).map_err(|_| {
        StorageError::infrastructure(
            "advance initiative revision",
            "the revision counter is larger than this store can hold",
        )
    })?;
    connection
        .execute(
            "INSERT INTO wayfind_initiative_revisions(initiative_id, revision) VALUES (?1, ?2) \
             ON CONFLICT(initiative_id) DO UPDATE SET revision = excluded.revision;",
            rusqlite::params![initiative_id.get(), raw],
        )
        .map_err(failed("advance initiative revision"))?;
    Ok(next)
}
