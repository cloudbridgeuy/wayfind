//! The v2 schema.
//!
//! v2 writes its own database file and never opens v1's, so nothing here has to
//! match what the Bash script wrote. What it does have to match is the storage
//! design, table for table and column for column.
//!
//! Every statement is `IF NOT EXISTS`, so running this against a database that
//! already has the schema changes nothing.

pub mod coordination;
pub mod graph;
pub mod search;

use std::fmt::Write as _;

use rusqlite::Connection;

/// Create every table the store needs, and guard the ones that never change.
///
/// The graph comes first: the coordination tables point at `projects` and
/// `initiatives`, so those have to exist before a foreign key can name them.
pub fn create(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(graph::DDL)?;
    connection.execute_batch(coordination::DDL)?;
    connection.execute_batch(search::DDL)?;
    connection.execute_batch(&guard_triggers())
}

/// A pair of triggers per immutable table, refusing UPDATE and DELETE.
///
/// The DDL is generated from [`graph::TABLES`] rather than written out, so a
/// table added to the immutable domain is guarded by construction. `immutable`
/// is the error token the abort raises, which is the one a caller reads back.
fn guard_triggers() -> String {
    let mut sql = String::new();
    for table in graph::TABLES {
        for event in ["UPDATE", "DELETE"] {
            let suffix = event.to_lowercase();
            let _ = write!(
                sql,
                "CREATE TRIGGER IF NOT EXISTS guard_{table}_{suffix}\n\
                 BEFORE {event} ON {table}\n\
                 BEGIN SELECT RAISE(ABORT, 'immutable'); END;\n"
            );
        }
    }
    sql
}
