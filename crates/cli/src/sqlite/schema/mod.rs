//! The v2 schema.
//!
//! v2 writes its own database file and never opens v1's, so nothing here has to
//! match what the Bash script wrote. What it does have to match is the storage
//! design, table for table and column for column.
//!
//! Every statement is `IF NOT EXISTS`, so running this against a database that
//! already has the schema changes nothing.

pub mod graph;

use rusqlite::Connection;

/// Create every table the store needs.
pub fn create(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(graph::DDL)
}
