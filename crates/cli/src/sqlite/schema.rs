//! The schema, exactly as the Bash script wrote it.
//!
//! Not a line of this differs from `ensure_database` in the script at commit
//! `0120d61`. That is the whole point: an operator upgrading from the script
//! keeps one database file, and a database created by either program has to be
//! indistinguishable to the other.
//!
//! Every statement is `IF NOT EXISTS`, so running this against a database the
//! script already made changes nothing at all.

use rusqlite::Connection;
use wayfind_core::{StorageError, StorageResult};

/// The tables, the search index, and the triggers that keep them together.
pub const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS projects (
  key TEXT PRIMARY KEY,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS initiatives (
  id INTEGER PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  name TEXT NOT NULL,
  destination TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'charting' CHECK(status IN ('charting', 'working', 'clear')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(project_key, name)
);
CREATE TABLE IF NOT EXISTS tickets (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  title TEXT NOT NULL,
  type TEXT NOT NULL CHECK(type IN ('grilling', 'research', 'prototype', 'task')),
  status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open', 'claimed', 'resolved', 'excluded')),
  question TEXT NOT NULL,
  resolution TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  resolved_at TEXT,
  UNIQUE(initiative_id, title)
);
CREATE TABLE IF NOT EXISTS ticket_dependencies (
  ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  blocker_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  PRIMARY KEY(ticket_id, blocker_id),
  CHECK(ticket_id != blocker_id)
);
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  initiative_id INTEGER REFERENCES initiatives(id),
  current_ticket_id INTEGER REFERENCES tickets(id),
  resolved_non_research_count INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'closed')),
  started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS ticket_claims (
  ticket_id INTEGER PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  claimed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  released_at TEXT
);
CREATE TABLE IF NOT EXISTS decisions (
  id INTEGER PRIMARY KEY,
  ticket_id INTEGER NOT NULL UNIQUE REFERENCES tickets(id),
  gist TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS fog_notes (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id) ON DELETE CASCADE,
  note TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS scope_exclusions (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id) ON DELETE CASCADE,
  note TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS attachments (
  id INTEGER PRIMARY KEY,
  ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  content TEXT NOT NULL,
  byte_size INTEGER NOT NULL,
  session_id TEXT REFERENCES sessions(id),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(ticket_id, name)
);
CREATE TABLE IF NOT EXISTS attachment_references (
  attachment_id INTEGER NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY(attachment_id, ticket_id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS ticket_search USING fts5(title, question, resolution);
CREATE TRIGGER IF NOT EXISTS tickets_ai AFTER INSERT ON tickets BEGIN
  INSERT INTO ticket_search(rowid, title, question, resolution)
  VALUES (new.id, new.title, new.question, coalesce(new.resolution, ''));
END;
CREATE TRIGGER IF NOT EXISTS tickets_ad AFTER DELETE ON tickets BEGIN
  DELETE FROM ticket_search WHERE rowid = old.id;
END;
CREATE TRIGGER IF NOT EXISTS tickets_au AFTER UPDATE OF title, question, resolution ON tickets BEGIN
  DELETE FROM ticket_search WHERE rowid = old.id;
  INSERT INTO ticket_search(rowid, title, question, resolution)
  VALUES (new.id, new.title, new.question, coalesce(new.resolution, ''));
END;
";

/// The two tables the adapter needs and the script never had.
///
/// Both are additive and both are namespaced, so a script that still opens the
/// same file keeps working and cannot collide with a table of its own. Neither
/// declares a foreign key on purpose: the script knows nothing about these rows,
/// and a constraint would let one of them refuse a delete the script is entitled
/// to make. A row left behind by such a delete is harmless — an identifier
/// counter that never goes backwards, and a revision nobody asks about.
pub const ADAPTER_SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS wayfind_initiative_revisions (
  initiative_id INTEGER PRIMARY KEY,
  revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS wayfind_id_sequences (
  scope TEXT PRIMARY KEY,
  next_id INTEGER NOT NULL
);
";

/// Create every table, index, and trigger that is not there yet.
pub fn create_schema(connection: &Connection) -> StorageResult<()> {
    connection
        .execute_batch(SCHEMA)
        .map_err(|error| StorageError::infrastructure("create schema", error.to_string()))
}

/// Create the adapter's own two tables if they are not there yet.
pub fn create_adapter_schema(connection: &Connection) -> StorageResult<()> {
    connection
        .execute_batch(ADAPTER_SCHEMA)
        .map_err(|error| StorageError::infrastructure("create adapter schema", error.to_string()))
}

/// Switch a newly created database to write-ahead logging.
///
/// Only `init` calls this. An existing database keeps whatever journal mode it
/// already has: the operator may have chosen it deliberately, and changing it
/// under them is not this program's business.
pub fn enable_write_ahead_logging(connection: &Connection) -> StorageResult<()> {
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| StorageError::infrastructure("set journal mode", error.to_string()))
}

/// Add the one column `CREATE TABLE IF NOT EXISTS` cannot add.
///
/// This is the whole of the compatibility story, and it is deliberately not a
/// migration framework. The script grew exactly one column after its first
/// release, the addition is additive and nullable, and running the check on a
/// database that already has the column does nothing. A general framework would
/// be more machinery than there is history to justify.
pub fn ensure_amended_at(connection: &Connection) -> StorageResult<()> {
    if !has_tickets_table(connection)? {
        // Nothing to add a column to. `create_schema` already includes it.
        return Ok(());
    }
    if has_column(connection, "tickets", "amended_at")? {
        return Ok(());
    }
    connection
        .execute_batch("ALTER TABLE tickets ADD COLUMN amended_at TEXT;")
        .map_err(|error| StorageError::infrastructure("add amended_at", error.to_string()))
}

fn has_tickets_table(connection: &Connection) -> StorageResult<bool> {
    connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'tickets';",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|error| StorageError::infrastructure("inspect schema", error.to_string()))
}

fn has_column(connection: &Connection, table: &str, column: &str) -> StorageResult<bool> {
    connection
        .query_row(
            "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2;",
            rusqlite::params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|error| StorageError::infrastructure("inspect columns", error.to_string()))
}
