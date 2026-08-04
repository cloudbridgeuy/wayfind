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

#[test]
fn creates_every_immutable_table() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    wayfind_cli::sqlite::schema::create(&conn).unwrap();
    for table in [
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
    ] {
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
