//! Getting a connection to the v2 store.
//!
//! Two ways in, and the difference between them is the whole point. `wayfind2
//! init` creates; every other command opens. An open that finds nothing says so
//! instead of leaving an empty database behind for the next command to be
//! confused by.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use wayfind_core::{
    error::{ErrorToken, Rejection},
    render::Field,
};

use crate::{
    error::{ShellError, ShellResult},
    sqlite::schema,
};

/// An open connection to a wayfind v2 database.
#[derive(Debug)]
pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    /// Create the database if it is absent, then open it.
    ///
    /// This is the only place that makes a directory or a file, and the only
    /// place that sets the journal mode. Write-ahead logging is a property of
    /// the file, so setting it once at creation is enough; a later command that
    /// set it again would be claiming an authority over the operator's file that
    /// it does not have.
    pub fn initialize(path: &Path) -> ShellResult<SqliteStore> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| ShellError::file("create", parent, error))?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        schema::create(&connection)?;
        Ok(SqliteStore { connection })
    }

    /// Open a database that already exists.
    ///
    /// A missing file is a refusal with a token, not a fault: the operator
    /// pointed at a store that was never created, and the answer is to create
    /// it.
    pub fn open(path: &Path) -> ShellResult<SqliteStore> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI;
        let connection = Connection::open_with_flags(path, flags).map_err(|_| missing(path))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        Ok(SqliteStore { connection })
    }

    /// The open connection, for the capabilities built on top of it.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// The refusal an absent store earns.
fn missing(path: &Path) -> ShellError {
    ShellError::Rejected(
        Rejection::new(ErrorToken::NotFound)
            .key("path", Field::Text(path.display().to_string()))
            .body("No store exists at that path. Run `wayfind2 init` to create it."),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn initializing_creates_the_file_its_parent_and_the_schema() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("nested/wayfind2.sqlite");
        let store = SqliteStore::initialize(&path).unwrap();
        assert!(path.exists());

        let tables: i64 = store
            .connection()
            .query_row(
                "select count(*) from sqlite_master where type = 'table' and name = 'result_nodes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
    }

    #[test]
    fn initializing_turns_on_write_ahead_logging_and_foreign_keys() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("wayfind2.sqlite");
        let store = SqliteStore::initialize(&path).unwrap();

        let mode: String = store
            .connection()
            .query_row("pragma journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        assert!(foreign_keys_on(&store));
    }

    #[test]
    fn initializing_twice_is_the_same_as_once() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("wayfind2.sqlite");
        SqliteStore::initialize(&path).unwrap();
        SqliteStore::initialize(&path).unwrap();
    }

    #[test]
    fn opening_an_existing_store_enforces_foreign_keys() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("wayfind2.sqlite");
        SqliteStore::initialize(&path).unwrap();

        let store = SqliteStore::open(&path).unwrap();
        assert!(foreign_keys_on(&store));
    }

    #[test]
    fn opening_a_store_that_is_not_there_says_which_path_and_how_to_make_it() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("wayfind2.sqlite");

        let error = SqliteStore::open(&path).unwrap_err();
        let rejection = error.rejection().expect("a missing store is a rejection");
        assert_eq!(rejection.token(), ErrorToken::NotFound);
        assert_eq!(
            rejection.keys(),
            [("path", Field::Text(path.display().to_string()))]
        );
        assert!(rejection
            .body_text()
            .is_some_and(|body| body.contains("wayfind2 init")));
        assert!(!path.exists(), "a failed open left a file behind");
    }

    fn foreign_keys_on(store: &SqliteStore) -> bool {
        store
            .connection()
            .query_row("pragma foreign_keys", [], |row| row.get::<_, i64>(0))
            .unwrap()
            == 1
    }
}
