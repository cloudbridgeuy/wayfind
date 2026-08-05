//! The read adapters slice A3 adds: one snapshot, accepted transitions
//! through a snapshot, one node, and prefix resolution.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use rusqlite::Connection;
use wayfind_cli::sqlite::graph_write::SqliteGraph;
use wayfind_cli::sqlite::schema;
use wayfind_core::command::graph::CreateInitiativeCommand;
use wayfind_core::id::{ProjectKey, RecordKind, SessionId, SnapshotOrdinal};
use wayfind_core::outcome::graph::CreateInitiativeOutcome;
use wayfind_core::storage::graph::{GraphAppender, GraphReader};
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

fn seeded(
    name: &str,
) -> (
    Connection,
    wayfind_core::id::InitiativeId,
    wayfind_core::id::RecordId,
) {
    let connection = Connection::open_in_memory().unwrap();
    schema::create(&connection).unwrap();
    let graph = SqliteGraph::new(&connection);

    let validated = validate_create(command(name)).unwrap();
    let destination = validated.destination_node.id;
    let created = match graph.create_initiative(validated).unwrap() {
        CreateInitiativeOutcome::Created(initiative) => initiative,
        CreateInitiativeOutcome::NameTaken { .. } => panic!("expected Created"),
    };
    (connection, created.id, destination)
}

#[test]
fn one_snapshot_reads_back_the_root() {
    let (connection, initiative, _destination) = seeded("Ship v2");
    let graph = SqliteGraph::new(&connection);

    let root = graph
        .snapshot(initiative, SnapshotOrdinal::new(1).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(root.ordinal.get(), 1);
    assert!(root.transition.is_none());

    assert!(graph
        .snapshot(initiative, SnapshotOrdinal::new(2).unwrap())
        .unwrap()
        .is_none());
}

#[test]
fn accepted_transitions_through_the_root_is_empty() {
    let (connection, initiative, _destination) = seeded("Ship v2");
    let graph = SqliteGraph::new(&connection);

    let transitions = graph
        .accepted_transitions(initiative, SnapshotOrdinal::new(1).unwrap())
        .unwrap();
    assert!(transitions.is_empty());
}

#[test]
fn a_node_is_read_back_by_hash() {
    let (connection, _initiative, destination) = seeded("Ship v2");
    let graph = SqliteGraph::new(&connection);

    let node = graph.node(&destination.hash()).unwrap().unwrap();
    assert_eq!(node.id, destination);
    assert_eq!(node.draft.title, "Ship v2");

    let missing = wayfind_core::id::Hash::parse_hex(&"0".repeat(64)).unwrap();
    assert!(graph.node(&missing).unwrap().is_none());
}

#[test]
fn resolve_prefix_finds_the_destination_node_by_a_short_hex_run() {
    let (connection, _initiative, destination) = seeded("Ship v2");
    let graph = SqliteGraph::new(&connection);

    let hex = destination.hash().to_hex();
    let matches = graph.resolve_prefix(RecordKind::Node, &hex[..4]).unwrap();
    assert_eq!(matches, vec![destination]);

    let none = graph.resolve_prefix(RecordKind::Node, "ffff").unwrap();
    assert!(none.is_empty());

    let wrong_kind = graph
        .resolve_prefix(RecordKind::Transition, &hex[..4])
        .unwrap();
    assert!(wrong_kind.is_empty());
}

fn wayfind2(home: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_wayfind2"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", home)
        .env_remove("WAYFIND_SESSION_ID")
        .output()
        .expect("run wayfind2")
}

fn front_matter(stdout: &[u8]) -> toml::Table {
    let document = String::from_utf8(stdout.to_vec()).unwrap();
    let fenced = document
        .strip_prefix("+++\n")
        .expect("document opens with a front-matter fence");
    let end = fenced.find("+++").expect("document closes the fence");
    fenced[..end].parse::<toml::Table>().unwrap()
}

#[test]
fn initiative_list_and_show_carry_the_same_facts_the_adapter_read() {
    let home = tempfile::tempdir().unwrap();
    assert!(wayfind2(home.path(), &["init"]).status.success());
    assert!(wayfind2(
        home.path(),
        &[
            "initiative",
            "create",
            "--name",
            "Ship v2",
            "--destination",
            "wayfind v2 in daily use",
            "--session",
            "s",
        ]
    )
    .status
    .success());

    let list = wayfind2(home.path(), &["initiative", "list"]);
    assert!(list.status.success());
    let list_matter = front_matter(&list.stdout);
    assert_eq!(list_matter["kind"].as_str(), Some("initiative-list"));
    assert_eq!(list_matter["count"].as_integer(), Some(1));

    let show = wayfind2(home.path(), &["initiative", "show", "1"]);
    assert!(show.status.success());
    let show_matter = front_matter(&show.stdout);
    assert_eq!(show_matter["kind"].as_str(), Some("initiative"));
    assert_eq!(show_matter["initiative"].as_integer(), Some(1));
    assert_eq!(show_matter["name"].as_str(), Some("Ship v2"));
    assert_eq!(show_matter["head"].as_str(), Some("S1"));

    let missing = wayfind2(home.path(), &["initiative", "show", "2"]);
    assert!(!missing.status.success());
    let error_matter = front_matter(&missing.stderr);
    assert_eq!(error_matter["kind"].as_str(), Some("error"));
    assert_eq!(error_matter["error"].as_str(), Some("not-found"));
}
