//! Reading the graph back, and allocating coordination identifiers — the two
//! capabilities the composition root needs alongside `GraphAppender` to build
//! a `Shell`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use rusqlite::Connection;
use wayfind_cli::sqlite::graph_write::SqliteGraph;
use wayfind_cli::sqlite::schema;
use wayfind_core::command::graph::CreateInitiativeCommand;
use wayfind_core::id::{ProjectKey, SessionId};
use wayfind_core::outcome::graph::CreateInitiativeOutcome;
use wayfind_core::storage::coordination::MutableIdAllocator;
use wayfind_core::storage::graph::{GraphAppender, GraphReader};
use wayfind_core::storage::values::IdScope;
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

#[test]
fn a_created_initiative_is_read_back_with_its_snapshot_and_root_member() {
    let connection = Connection::open_in_memory().unwrap();
    schema::create(&connection).unwrap();
    let graph = SqliteGraph::new(&connection);

    let validated = validate_create(command("Ship v2")).unwrap();
    let destination = validated.destination_node.id;
    let created = match graph.create_initiative(validated).unwrap() {
        CreateInitiativeOutcome::Created(initiative) => initiative,
        CreateInitiativeOutcome::NameTaken { .. } => panic!("expected Created"),
    };

    let read_back = graph.initiative(created.id).unwrap().unwrap();
    assert_eq!(read_back.id, created.id);
    assert_eq!(read_back.name, "Ship v2");

    let listed = graph.initiatives(&created.project).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);

    let snapshots = graph.snapshots(created.id).unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].ordinal.get(), 1);
    assert!(snapshots[0].transition.is_none());

    let members = graph.root_members(created.id).unwrap();
    assert_eq!(members, vec![destination]);
}

#[test]
fn an_absent_initiative_reads_as_none() {
    let connection = Connection::open_in_memory().unwrap();
    schema::create(&connection).unwrap();
    let graph = SqliteGraph::new(&connection);

    let validated = validate_create(command("Ship v2")).unwrap();
    let created = match graph.create_initiative(validated).unwrap() {
        CreateInitiativeOutcome::Created(initiative) => initiative,
        CreateInitiativeOutcome::NameTaken { .. } => panic!("expected Created"),
    };

    let other = wayfind_core::id::InitiativeId::new(created.id.get() + 1).unwrap();
    assert!(graph.initiative(other).unwrap().is_none());
}

#[test]
fn allocation_is_monotonic_per_scope_and_independent_across_scopes() {
    let connection = Connection::open_in_memory().unwrap();
    schema::create(&connection).unwrap();
    let graph = SqliteGraph::new(&connection);

    let first_ticket = graph.allocate(IdScope::Ticket).unwrap().ticket().unwrap();
    let second_ticket = graph.allocate(IdScope::Ticket).unwrap().ticket().unwrap();
    assert_eq!(first_ticket.get(), 1);
    assert_eq!(second_ticket.get(), 2);

    let first_run = graph.allocate(IdScope::Run).unwrap().run().unwrap();
    assert_eq!(first_run.get(), 1);
}
