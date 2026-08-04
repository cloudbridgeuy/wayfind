//! The schema a fresh v2 store is created with.
//!
//! These are checks against a real SQLite connection rather than against a
//! string: what matters is what the file ends up holding, not what the DDL
//! constant happens to spell.

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn count_object(conn: &rusqlite::Connection, kind: &str, name: &str) -> i64 {
    conn.query_row(
        "select count(*) from sqlite_master where type=?1 and name=?2",
        rusqlite::params![kind, name],
        |row| row.get(0),
    )
    .unwrap()
}

/// The immutable tables, spelled out rather than read from the crate, so the
/// test fails if a table is dropped from the list the DDL is generated from.
const IMMUTABLE_TABLES: [&str; 13] = [
    "projects",
    "initiatives",
    "result_nodes",
    "transitions",
    "transition_inputs",
    "transition_outputs",
    "connections",
    "artifacts",
    "artifact_references",
    "import_provenance",
    "import_members",
    "snapshots",
    "root_members",
];

/// The mutable tables, spelled out for the same reason.
const COORDINATION_TABLES: [&str; 16] = [
    "sessions",
    "node_claims",
    "runs",
    "questions",
    "question_dependencies",
    "question_claims",
    "decisions",
    "fog_notes",
    "scope_exclusions",
    "run_attachments",
    "attachment_references",
    "tickets",
    "ticket_links",
    "run_revisions",
    "id_sequences",
    "legacy_initiatives",
];

#[test]
fn creates_every_immutable_table() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    for table in IMMUTABLE_TABLES {
        assert_eq!(
            count_object(&conn, "table", table),
            1,
            "missing table {table}"
        );
    }
}

#[test]
fn creating_the_schema_twice_changes_nothing() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    assert_eq!(count_object(&conn, "table", "result_nodes"), 1);
}

#[test]
fn an_initiative_has_no_status_and_a_snapshot_has_no_head_pointer() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    assert!(!has_column(&conn, "initiatives", "status"));
    assert!(!has_column(&conn, "snapshots", "is_head"));
    assert!(!has_column(&conn, "snapshots", "head"));
}

#[test]
fn one_name_can_be_used_once_per_project() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    conn.execute(
        "insert into projects (key, created_at) values ('/work/repo', '2026-08-03T00:00:00Z')",
        [],
    )
    .unwrap();
    let insert = "insert into initiatives (id, project_key, name, destination, created_at) \
                  values (?1, '/work/repo', 'the name', 'somewhere', '2026-08-03T00:00:00Z')";
    conn.execute(insert, [1]).unwrap();
    assert!(conn.execute(insert, [2]).is_err());
}

#[test]
fn every_immutable_table_carries_both_guard_triggers() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    for table in IMMUTABLE_TABLES {
        for event in ["update", "delete"] {
            let name = format!("guard_{table}_{event}");
            assert_eq!(count_object(&conn, "trigger", &name), 1, "missing {name}");
        }
    }
}

#[test]
fn immutable_tables_refuse_update_and_delete() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();

    // Every column takes the same value, so each foreign key resolves against
    // the row seeded into the table before it. The point is that the guard
    // fires on a row that is already there, not that the row means anything.
    for table in IMMUTABLE_TABLES {
        let columns = columns_of(&conn, table);
        let names = columns.join(", ");
        let values = vec!["'1'"; columns.len()].join(", ");
        conn.execute(
            &format!("insert into {table} ({names}) values ({values})"),
            [],
        )
        .unwrap_or_else(|error| panic!("cannot seed {table}: {error}"));

        let first = &columns[0];
        let update = conn.execute(&format!("update {table} set {first} = '2'"), []);
        assert!(update.is_err(), "{table} accepted an update");
        assert!(
            update.unwrap_err().to_string().contains("immutable"),
            "{table} refused an update without saying why"
        );

        let delete = conn.execute(&format!("delete from {table}"), []);
        assert!(delete.is_err(), "{table} accepted a delete");
        assert!(
            delete.unwrap_err().to_string().contains("immutable"),
            "{table} refused a delete without saying why"
        );
    }
}

#[test]
fn creates_every_coordination_table() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    for table in COORDINATION_TABLES {
        assert_eq!(
            count_object(&conn, "table", table),
            1,
            "missing table {table}"
        );
    }
}

#[test]
fn no_coordination_table_carries_a_guard_trigger() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    for table in COORDINATION_TABLES {
        for event in ["update", "delete"] {
            let name = format!("guard_{table}_{event}");
            assert_eq!(
                count_object(&conn, "trigger", &name),
                0,
                "{table} is mutable by design but carries {name}"
            );
        }
    }
}

#[test]
fn a_question_cannot_block_itself() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    // The CHECK is the subject, so no run and no question is set up for the
    // foreign keys to resolve against.
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    let insert = "insert into question_dependencies (question_id, blocker_id) values (?1, ?2)";
    conn.execute(insert, [1, 2]).unwrap();
    assert!(conn.execute(insert, [1, 1]).is_err());
}

#[test]
fn a_ticket_takes_only_the_vocabularies() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    // Same again: the vocabularies are the subject, not the project link.
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    let insert = "insert into tickets \
                  (id, project_key, title, description, priority, status, created_at, updated_at) \
                  values (?1, '/work/repo', 'a title', 'a description', ?2, ?3, \
                  '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z')";
    conn.execute(insert, rusqlite::params![1, "normal", "open"])
        .unwrap();
    assert!(conn
        .execute(insert, rusqlite::params![2, "critical", "open"])
        .is_err());
    assert!(conn
        .execute(insert, rusqlite::params![3, "normal", "closed"])
        .is_err());
}

#[test]
fn a_ticket_link_does_not_point_its_node_at_the_graph() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    let mut statement = conn
        .prepare("pragma foreign_key_list(ticket_links)")
        .unwrap();
    let mut rows = statement.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let column: String = row.get(3).unwrap();
        assert_ne!(column, "node_hash", "node_hash carries a foreign key");
    }
}

fn columns_of(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut statement = conn
        .prepare(&format!("pragma table_info({table})"))
        .unwrap();
    let mut rows = statement.query([]).unwrap();
    let mut names = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        names.push(row.get::<_, String>(1).unwrap());
    }
    names
}

fn has_column(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let mut statement = conn
        .prepare(&format!("pragma table_info({table})"))
        .unwrap();
    let mut rows = statement.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let name: String = row.get(1).unwrap();
        if name == column {
            return true;
        }
    }
    false
}
