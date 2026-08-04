//! T2: creating a new initiative's first record.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use rusqlite::Connection;
use wayfind_cli::sqlite::graph_write::SqliteGraph;
use wayfind_cli::sqlite::schema;
use wayfind_core::command::graph::CreateInitiativeCommand;
use wayfind_core::id::{ProjectKey, SessionId};
use wayfind_core::outcome::graph::CreateInitiativeOutcome;
use wayfind_core::storage::graph::GraphAppender;
use wayfind_core::time::Timestamp;
use wayfind_core::validate::initiative::validate_create;

fn command(name: &str) -> CreateInitiativeCommand {
    CreateInitiativeCommand {
        project: ProjectKey::new("/repo").unwrap(),
        name: name.into(),
        destination: "wayfind v2 in daily use".into(),
        notes: None,
        created_by: SessionId::new("s").unwrap(),
        now: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
    }
}

fn row_count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn a_duplicate_name_is_name_taken_and_writes_nothing_new() {
    let connection = Connection::open_in_memory().unwrap();
    schema::create(&connection).unwrap();
    let graph = SqliteGraph::new(&connection);

    let first = graph
        .create_initiative(validate_create(command("Ship v2")).unwrap())
        .unwrap();
    let created = match first {
        CreateInitiativeOutcome::Created(initiative) => initiative,
        CreateInitiativeOutcome::NameTaken { .. } => panic!("expected Created"),
    };
    assert_eq!(created.id.get(), 1);
    assert_eq!(row_count(&connection, "initiatives"), 1);

    let second = graph
        .create_initiative(validate_create(command("Ship v2")).unwrap())
        .unwrap();
    match second {
        CreateInitiativeOutcome::NameTaken { existing } => {
            assert_eq!(existing, created.id);
        }
        CreateInitiativeOutcome::Created(_) => panic!("expected NameTaken"),
    }
    assert_eq!(row_count(&connection, "initiatives"), 1);
    assert_eq!(row_count(&connection, "result_nodes"), 1);
    assert_eq!(row_count(&connection, "snapshots"), 1);
    assert_eq!(row_count(&connection, "root_members"), 1);
}
