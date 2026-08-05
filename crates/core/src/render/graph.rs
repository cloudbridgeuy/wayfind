//! The `snapshot-list` and `snapshot` documents: what `snapshot list` and
//! `snapshot show` print.

use crate::derive::GraphState;
use crate::id::{InitiativeId, SnapshotOrdinal};
use crate::record::{ResultNode, Snapshot, Transition};
use crate::render::FrontMatter;

/// Render every snapshot of an initiative, in ordinal order.
pub fn snapshot_list_document(initiative: InitiativeId, snapshots: &[Snapshot]) -> String {
    let head = snapshots
        .iter()
        .map(|snapshot| snapshot.ordinal.get())
        .max();

    let mut out = FrontMatter::new("snapshot-list")
        .number("initiative", initiative.get())
        .number("count", snapshots.len() as i64)
        .field(
            "head",
            crate::render::Field::Text(match head {
                Some(ordinal) => format!("S{ordinal}"),
                None => String::new(),
            }),
        )
        .render();

    out.push('\n');
    for snapshot in snapshots {
        out.push_str(&format!("## {}\n\n", snapshot.ordinal));
        out.push_str(&format!(
            "- transition: {}\n",
            match &snapshot.transition {
                Some(id) => id.abbreviated(),
                None => "none".to_string(),
            }
        ));
        out.push_str(&format!(
            "- base: {}\n",
            match snapshot.declared_base {
                Some(base) => base.to_string(),
                None => "none".to_string(),
            }
        ));
        out.push_str(&format!("- chain_hash: {}\n\n", snapshot.chain_hash));
    }

    out
}

/// Render one snapshot and its full membership.
pub fn snapshot_document(snapshot: &Snapshot, members: &GraphState) -> String {
    let mut out = FrontMatter::new("snapshot")
        .number("initiative", snapshot.initiative.get())
        .text("snapshot", snapshot.ordinal.to_string())
        .text("chain_hash", snapshot.chain_hash.to_string())
        .ids(
            "nodes",
            members.nodes().iter().map(ToString::to_string).collect(),
        )
        .ids(
            "transitions",
            members
                .transitions()
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
        .ids(
            "connections",
            members
                .connections()
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
        .ids(
            "artifacts",
            members
                .artifacts()
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
        .render();

    out.push('\n');
    out.push_str(&format!("# Snapshot {}\n\n", snapshot.ordinal));
    out.push_str(&format!(
        "{} nodes, {} transitions, {} connections, {} artifacts.\n",
        members.nodes().len(),
        members.transitions().len(),
        members.connections().len(),
        members.artifacts().len(),
    ));

    out
}

/// Render the graph at one snapshot: its members' counts, then a section per
/// kind naming each member by its abbreviation and its title or summary.
///
/// `nodes` and `transitions` carry the full records of `members`' node and
/// transition members — `members` alone only has their ids, and the body
/// needs the text a reader recognizes a record by.
pub fn graph_document(
    members: &GraphState,
    initiative: InitiativeId,
    at: SnapshotOrdinal,
    nodes: &[ResultNode],
    transitions: &[Transition],
) -> String {
    let mut out = FrontMatter::new("graph")
        .number("initiative", initiative.get())
        .text("snapshot", at.to_string())
        .number("nodes", members.nodes().len() as i64)
        .number("transitions", members.transitions().len() as i64)
        .number("connections", members.connections().len() as i64)
        .render();

    out.push('\n');
    out.push_str("## Nodes\n\n");
    for node in nodes {
        out.push_str(&format!(
            "- {} {}\n",
            node.id.abbreviated(),
            node.draft.title
        ));
    }

    out.push_str("\n## Transitions\n\n");
    for transition in transitions {
        out.push_str(&format!(
            "- {} {}\n",
            transition.id.abbreviated(),
            transition.draft.summary
        ));
    }

    out.push_str("\n## Connections\n\n");
    for connection in members.connections() {
        out.push_str(&format!("- {}\n", connection.abbreviated()));
    }

    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{graph_document, snapshot_document, snapshot_list_document};
    use crate::derive::members_at;
    use crate::id::{Hash, InitiativeId, RecordId, SnapshotOrdinal};
    use crate::record::Snapshot;
    use crate::time::Timestamp;

    fn snapshot(ordinal: u32, transition: Option<RecordId>) -> Snapshot {
        Snapshot {
            initiative: InitiativeId::new(1).unwrap(),
            ordinal: SnapshotOrdinal::new(ordinal).unwrap(),
            transition,
            declared_base: None,
            chain_hash: Hash::parse_hex(&"a".repeat(64)).unwrap(),
            created_at: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
        }
    }

    #[test]
    fn the_list_document_carries_the_initiative_the_count_the_head_and_one_section_per_snapshot() {
        let snapshots = vec![snapshot(1, None)];
        let out = snapshot_list_document(InitiativeId::new(1).unwrap(), &snapshots);

        assert!(out.contains("kind = \"snapshot-list\""));
        assert!(out.contains("initiative = 1"));
        assert!(out.contains("count = 1"));
        assert!(out.contains("head = \"S1\""));
        assert!(out.contains("## S1"));
        assert!(out.contains("- transition: none"));
    }

    #[test]
    fn the_show_document_carries_the_full_chain_hash_and_the_member_ids() {
        let id = RecordId::from_str(
            "R-0000000000000000000000000000000000000000000000000000000000000abc",
        )
        .unwrap();

        let root = vec![id];
        let members = members_at(&root, &[], SnapshotOrdinal::new(1).unwrap());

        let snap = snapshot(1, None);
        let out = snapshot_document(&snap, &members);

        assert!(out.contains("kind = \"snapshot\""));
        assert!(out.contains(&format!("chain_hash = \"{}\"", "a".repeat(64))));
        assert!(out.contains(&format!("nodes = [\"{id}\"]")));
    }

    #[test]
    fn the_graph_document_lists_one_node_and_no_transition_after_a_root_snapshot() {
        use crate::id::{RecordKind, SessionId};
        use crate::kinds::NodeKind;
        use crate::record::{NodeDraft, ResultNode};

        let id = RecordId::new(RecordKind::Node, Hash::parse_hex(&"b".repeat(64)).unwrap());
        let node = ResultNode {
            id,
            draft: NodeDraft {
                node_kind: NodeKind::Destination,
                title: "Ship v2".into(),
                summary: None,
                content: "wayfind v2 in daily use".into(),
                created_at: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
                created_by: SessionId::new("s").unwrap(),
            },
        };

        let members = members_at(&[id], &[], SnapshotOrdinal::new(1).unwrap());
        let out = graph_document(
            &members,
            InitiativeId::new(1).unwrap(),
            SnapshotOrdinal::new(1).unwrap(),
            &[node],
            &[],
        );

        assert!(out.contains("kind = \"graph\""));
        assert!(out.contains("nodes = 1"));
        assert!(out.contains("transitions = 0"));
        assert!(out.contains(&format!("{} Ship v2", id.abbreviated())));
    }
}
