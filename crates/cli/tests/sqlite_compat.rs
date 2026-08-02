//! Reading a database the Bash script wrote.
//!
//! Every test here starts from `fixtures/create-bash-fixture.sql`, which is the
//! script's own schema and a set of deliberately awkward rows. The database is
//! rebuilt in a temporary directory for each test. Nothing in this file ever
//! names the operator's real database, and nothing here writes outside the
//! temporary directory it made.
//!
//! What is being proved is narrow and important: the port opens that file, sees
//! what the script saw, and leaves it in a state the script would still accept.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tempfile::TempDir;
use wayfind_cli::sqlite::{row, SqliteStorage};
use wayfind_core::{
    PersistedInitiativeStatus, SessionId, SessionState, TicketId, TicketState, TicketType,
};

/// The script's schema and rows, as SQL.
const FIXTURE: &str = include_str!("fixtures/create-bash-fixture.sql");

/// Build the fixture database inside a fresh temporary directory.
///
/// The directory is returned alongside the path because dropping it deletes
/// everything in it — including the database under test.
fn fixture() -> (TempDir, PathBuf) {
    let directory = TempDir::new().expect("make a temporary directory");
    let path = directory.path().join("wayfind.sqlite");
    let connection = Connection::open(&path).expect("create the fixture database");
    // Set write-ahead logging first, exactly as `ensure_database` does. It has
    // to be its own statement because it answers with a row.
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .expect("set write-ahead logging");
    connection
        .execute_batch(FIXTURE)
        .expect("apply the fixture SQL");
    drop(connection);
    (directory, path)
}

/// The journal mode SQLite reports for an open database.
fn journal_mode(connection: &Connection) -> String {
    connection
        .query_row("PRAGMA journal_mode;", [], |row| row.get::<_, String>(0))
        .expect("read the journal mode")
}

/// Every ticket id the search index returns for one query, in rank order.
fn search(connection: &Connection, query: &str) -> Vec<(i64, String)> {
    let mut statement = connection
        .prepare(
            "SELECT rowid, snippet(ticket_search, -1, '[', ']', '…', 8) \
             FROM ticket_search WHERE ticket_search MATCH ?1 ORDER BY rank;",
        )
        .expect("prepare the search");
    let rows = statement
        .query_map([query], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("run the search")
        .collect::<Result<Vec<(i64, String)>, _>>()
        .expect("read the hits");
    rows
}

/// Rows reported by SQLite's own referential integrity sweep.
fn broken_references(connection: &Connection) -> usize {
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check;")
        .expect("prepare the integrity check");
    let count = statement
        .query_map([], |_| Ok(()))
        .expect("run the integrity check")
        .count();
    count
}

// ---------------------------------------------------------------------------
// The file itself
// ---------------------------------------------------------------------------

#[test]
fn opening_a_script_database_leaves_write_ahead_logging_alone() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    assert_eq!(journal_mode(storage.connection()), "wal");
}

#[test]
fn opening_refuses_a_path_with_no_database() {
    let directory = TempDir::new().expect("make a temporary directory");
    let missing = directory.path().join("absent.sqlite");
    let error = SqliteStorage::open(&missing).expect_err("a missing database is refused");
    assert!(
        error.to_string().contains("wayfind init"),
        "the refusal should say how to make one, got: {error}"
    );
    assert!(
        !missing.exists(),
        "a mistyped path must not leave an empty database behind"
    );
}

#[test]
fn initializing_creates_the_directory_the_schema_and_the_journal_mode() {
    let directory = TempDir::new().expect("make a temporary directory");
    let path = directory.path().join("nested/deeper/wayfind.sqlite");
    let storage = SqliteStorage::initialize(&path).expect("create a database");
    assert_eq!(journal_mode(storage.connection()), "wal");
    let tables: i64 = storage
        .connection()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN \
             ('projects', 'initiatives', 'tickets', 'ticket_dependencies', 'sessions', \
              'ticket_claims', 'decisions', 'fog_notes', 'scope_exclusions', 'attachments', \
              'attachment_references');",
            [],
            |row| row.get(0),
        )
        .expect("count the tables");
    assert_eq!(tables, 11);
}

#[test]
fn initializing_twice_changes_nothing() {
    let directory = TempDir::new().expect("make a temporary directory");
    let path = directory.path().join("wayfind.sqlite");
    drop(SqliteStorage::initialize(&path).expect("create a database"));
    let storage = SqliteStorage::initialize(&path).expect("open it again");
    assert_eq!(broken_references(storage.connection()), 0);
}

#[test]
fn the_script_database_holds_no_broken_references() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    assert_eq!(broken_references(storage.connection()), 0);
}

#[test]
fn the_amended_at_column_is_added_only_when_it_is_absent() {
    let directory = TempDir::new().expect("make a temporary directory");
    let path = directory.path().join("wayfind.sqlite");

    // Build the schema as the script's first release did: no `amended_at`.
    let connection = Connection::open(&path).expect("create the database");
    connection
        .execute_batch(
            "CREATE TABLE tickets (
               id INTEGER PRIMARY KEY,
               initiative_id INTEGER NOT NULL,
               title TEXT NOT NULL,
               type TEXT NOT NULL,
               status TEXT NOT NULL,
               question TEXT NOT NULL,
               resolution TEXT,
               created_at TEXT NOT NULL,
               resolved_at TEXT
             );",
        )
        .expect("create the older tickets table");
    drop(connection);

    let storage = SqliteStorage::open(&path).expect("open the older database");
    let present: i64 = storage
        .connection()
        .query_row(
            "SELECT count(*) FROM pragma_table_info('tickets') WHERE name = 'amended_at';",
            [],
            |row| row.get(0),
        )
        .expect("look for the column");
    assert_eq!(present, 1, "the column should have been added");

    // Opening again must not try to add it a second time.
    drop(storage);
    SqliteStorage::open(&path).expect("open the migrated database");
}

// ---------------------------------------------------------------------------
// The search index and its triggers
// ---------------------------------------------------------------------------

#[test]
fn full_text_search_answers_a_bound_query_with_a_snippet() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    let hits = search(storage.connection(), "backend");
    assert_eq!(
        hits.len(),
        1,
        "one ticket mentions the backend, got {hits:?}"
    );
    let (id, snippet) = &hits[0];
    assert_eq!(*id, 1);
    assert!(
        snippet.contains('[') && snippet.contains(']'),
        "the snippet should mark the match, got: {snippet}"
    );
}

#[test]
fn search_reads_text_the_triggers_indexed_at_insert_time() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    // Non-Latin text and a phrase in quotes both went in through the trigger.
    assert_eq!(
        search(storage.connection(), "completions")
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![5]
    );
}

#[test]
fn updating_a_ticket_makes_the_new_text_searchable_and_the_old_text_gone() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");

    assert_eq!(
        search(storage.connection(), "diagram")
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![3],
        "the original question should be indexed"
    );

    storage
        .connection()
        .execute(
            "UPDATE tickets SET question = ?1 WHERE id = ?2;",
            rusqlite::params!["How are the swimlanes arranged?", 3_i64],
        )
        .expect("rewrite the question");

    assert!(
        search(storage.connection(), "diagram").is_empty(),
        "the update trigger should have removed the old text"
    );
    assert_eq!(
        search(storage.connection(), "swimlanes")
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![3],
        "the update trigger should have indexed the new text"
    );
}

#[test]
fn deleting_a_ticket_removes_it_from_the_index() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    storage
        .connection()
        .execute("DELETE FROM tickets WHERE id = ?1;", [5_i64])
        .expect("delete the excluded ticket");
    assert!(
        search(storage.connection(), "completions").is_empty(),
        "the delete trigger should have emptied the index row"
    );
    assert_eq!(broken_references(storage.connection()), 0);
}

// ---------------------------------------------------------------------------
// Row parsing
// ---------------------------------------------------------------------------

/// The ticket query every reader uses, joined to the one live claim.
const TICKET_QUERY: &str = "\
SELECT t.id, t.initiative_id, t.title, t.type, t.status, t.question, t.resolution,
       t.created_at, t.resolved_at, t.amended_at,
       c.session_id AS claim_session_id, c.claimed_at AS claimed_at
FROM tickets t
LEFT JOIN ticket_claims c ON c.ticket_id = t.id AND c.released_at IS NULL
WHERE t.id = ?1;";

fn ticket(path: &Path, id: i64) -> wayfind_core::Ticket {
    let storage = SqliteStorage::open(path).expect("open the fixture");
    let parsed = storage
        .connection()
        .query_row(TICKET_QUERY, [id], |row| Ok(row::parse_ticket(row)))
        .expect("read the ticket");
    parsed.expect("parse the ticket")
}

#[test]
fn a_resolved_and_amended_ticket_keeps_both_times_and_its_resolution() {
    let (_directory, path) = fixture();
    let parsed = ticket(&path, 1);
    assert_eq!(parsed.ticket_type, TicketType::Grilling);
    assert!(
        matches!(
            parsed.state,
            TicketState::Resolved {
                amended_at: Some(_),
                ..
            }
        ),
        "ticket 1 was resolved and later amended, got {:?}",
        parsed.state
    );
    assert!(parsed
        .state
        .resolution()
        .expect("a resolved ticket has a resolution")
        .contains("SQLite, bundled."));
    assert_eq!(
        parsed.state.claimant(),
        None,
        "a released claim is not live"
    );
}

#[test]
fn a_claimed_ticket_names_the_session_that_holds_it() {
    let (_directory, path) = fixture();
    let parsed = ticket(&path, 3);
    let claimant = parsed.state.claimant().expect("ticket 3 is held");
    assert_eq!(claimant.as_str(), "session-holding");
}

#[test]
fn an_open_ticket_has_neither_a_claim_nor_a_resolution() {
    let (_directory, path) = fixture();
    let parsed = ticket(&path, 4);
    assert!(matches!(parsed.state, TicketState::Open));
    assert_eq!(parsed.state.resolution(), None);
}

#[test]
fn an_excluded_ticket_parses_as_excluded() {
    let (_directory, path) = fixture();
    let parsed = ticket(&path, 5);
    assert_eq!(parsed.state.as_status_str(), "excluded");
}

#[test]
fn multiline_text_survives_the_round_trip_byte_for_byte() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    let initiative = storage
        .connection()
        .query_row(
            "SELECT id, project_key, name, destination, notes, status, created_at \
             FROM initiatives WHERE id = ?1;",
            [1_i64],
            |row| Ok(row::parse_initiative(row)),
        )
        .expect("read the initiative")
        .expect("parse the initiative");
    assert_eq!(initiative.status, PersistedInitiativeStatus::Working);
    assert_eq!(
        initiative.notes,
        "Notes across lines.\nThe second line has a \"quoted\" phrase and a\ttab.\nThe third line is 日本語."
    );
}

#[test]
fn a_session_holding_a_ticket_parses_as_holding_it() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    let mut statement = storage
        .connection()
        .prepare(
            "SELECT id, project_key, initiative_id, current_ticket_id, \
             resolved_non_research_count, status, started_at, last_seen_at \
             FROM sessions ORDER BY id;",
        )
        .expect("prepare the session query");
    let sessions = statement
        .query_map([], |row| Ok(row::parse_session(row)))
        .expect("run the session query")
        .collect::<Result<Vec<_>, _>>()
        .expect("read the sessions")
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("parse the sessions");

    assert_eq!(sessions.len(), 3);
    let holding = &sessions[1];
    assert_eq!(holding.id.as_str(), "session-holding");
    assert_eq!(
        holding.state.held_ticket().map(TicketId::get),
        Some(3),
        "session-holding is working on ticket 3"
    );

    let closed = &sessions[0];
    assert_eq!(closed.id.as_str(), "session-closed");
    assert!(matches!(closed.state, SessionState::Closed));

    let spent = &sessions[2];
    assert_eq!(spent.id.as_str(), "session-spent");
    assert!(spent.has_spent_non_research_budget());
}

#[test]
fn dependencies_attachments_and_references_parse() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    let connection = storage.connection();

    let mut statement = connection
        .prepare("SELECT ticket_id, blocker_id FROM ticket_dependencies ORDER BY ticket_id;")
        .expect("prepare the dependency query");
    let edges = statement
        .query_map([], |row| Ok(row::parse_dependency(row)))
        .expect("run the dependency query")
        .collect::<Result<Vec<_>, _>>()
        .expect("read the edges")
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("parse the edges");
    assert_eq!(
        edges
            .iter()
            .map(|edge| (edge.ticket_id().get(), edge.blocker_id().get()))
            .collect::<Vec<_>>(),
        vec![(3, 1), (4, 3)]
    );

    let attachment = connection
        .query_row(
            "SELECT id, ticket_id, name, description, byte_size, session_id, created_at \
             FROM attachments WHERE id = ?1;",
            [1_i64],
            |row| Ok(row::parse_attachment_metadata(row)),
        )
        .expect("read the attachment")
        .expect("parse the attachment");
    assert_eq!(attachment.name, "backend-comparison.md");
    assert_eq!(attachment.byte_size, 138);
    assert_eq!(
        attachment.session_id.as_ref().map(SessionId::as_str),
        Some("session-spent")
    );

    let reference = connection
        .query_row(
            "SELECT attachment_id, ticket_id, created_at FROM attachment_references \
             WHERE attachment_id = ?1;",
            [1_i64],
            |row| Ok(row::parse_attachment_reference(row)),
        )
        .expect("read the reference")
        .expect("parse the reference");
    assert_eq!(reference.ticket_id.get(), 3);
}

#[test]
fn a_claim_row_missing_its_time_is_reported_as_corrupt() {
    let (_directory, path) = fixture();
    let storage = SqliteStorage::open(&path).expect("open the fixture");
    let outcome = storage
        .connection()
        .query_row(
            "SELECT t.id, t.initiative_id, t.title, t.type, t.status, t.question, \
                    t.resolution, t.created_at, t.resolved_at, t.amended_at, \
                    'session-holding' AS claim_session_id, NULL AS claimed_at \
             FROM tickets t WHERE t.id = ?1;",
            [3_i64],
            |row| Ok(row::parse_ticket(row)),
        )
        .expect("read the row");
    let error = outcome.expect_err("half a claim is impossible");
    assert!(
        matches!(error, wayfind_core::StorageError::CorruptData(_)),
        "an impossible stored combination is corrupt data, got: {error}"
    );
}
