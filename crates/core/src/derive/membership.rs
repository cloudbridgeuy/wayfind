//! The membership rule: which records belong to a snapshot.
//!
//! Membership is not stored — it is derived, so it can never drift out of
//! agreement with the graph it describes. Per [S2 Detail
//! S3](../../../../../.claude/designs/2026-08-03-wayfind-v2-storage-design.md):
//! the members of `S(n)` are an initiative's root members, plus — for each
//! snapshot 2 through n — its accepted transition, that transition's output
//! nodes, that transition's connections, and every artifact those records
//! reference. Inputs are already members by validation, so they are never
//! added again.

use crate::id::{RecordId, RecordKind, SnapshotOrdinal};
use crate::record::Transition;

/// The complete membership of one snapshot, categorized by record kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphState {
    nodes: Vec<RecordId>,
    transitions: Vec<RecordId>,
    connections: Vec<RecordId>,
    artifacts: Vec<RecordId>,
}

impl GraphState {
    /// The member nodes.
    pub fn nodes(&self) -> &[RecordId] {
        &self.nodes
    }

    /// The member transitions.
    pub fn transitions(&self) -> &[RecordId] {
        &self.transitions
    }

    /// The member connections.
    pub fn connections(&self) -> &[RecordId] {
        &self.connections
    }

    /// The member artifacts.
    pub fn artifacts(&self) -> &[RecordId] {
        &self.artifacts
    }

    /// Whether a record belongs to this snapshot.
    pub fn contains(&self, id: &RecordId) -> bool {
        match id.kind() {
            RecordKind::Node => self.nodes.contains(id),
            RecordKind::Transition => self.transitions.contains(id),
            RecordKind::Connection => self.connections.contains(id),
            RecordKind::Artifact => self.artifacts.contains(id),
        }
    }

    fn add(&mut self, id: RecordId) {
        match id.kind() {
            RecordKind::Node => self.nodes.push(id),
            RecordKind::Transition => self.transitions.push(id),
            RecordKind::Connection => self.connections.push(id),
            RecordKind::Artifact => self.artifacts.push(id),
        }
    }
}

/// Derive the membership of `S(through)`.
///
/// `transitions` is an initiative's accepted transitions in snapshot order —
/// `transitions[0]` is the transition accepted into snapshot 2, `[1]` into
/// snapshot 3, and so on. `through` selects how many of them are in play; a
/// root snapshot (`through == S1`) selects none.
pub fn members_at(
    root: &[RecordId],
    transitions: &[Transition],
    through: SnapshotOrdinal,
) -> GraphState {
    let mut state = GraphState::default();
    for id in root {
        state.add(*id);
    }

    let accepted = through.get().saturating_sub(1) as usize;
    for transition in transitions.iter().take(accepted) {
        state.add(transition.id);
        for output in &transition.draft.outputs {
            state.add(*output);
        }
        // Connections and the artifacts they reference have no write path
        // yet — slice B1 extends this loop when they do.
    }

    state
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::members_at;
    use crate::id::{Hash, RecordId, RecordKind, SessionId, SnapshotOrdinal};
    use crate::kinds::TransitionKind;
    use crate::record::{Transition, TransitionDraft};
    use crate::time::Timestamp;

    fn node(hex_prefix: &str) -> RecordId {
        let hex = format!("{hex_prefix}{}", "0".repeat(64 - hex_prefix.len()));
        RecordId::new(RecordKind::Node, Hash::parse_hex(&hex).unwrap())
    }

    fn transition_id(hex_prefix: &str) -> RecordId {
        let hex = format!("{hex_prefix}{}", "0".repeat(64 - hex_prefix.len()));
        RecordId::new(RecordKind::Transition, Hash::parse_hex(&hex).unwrap())
    }

    fn transition(hex_prefix: &str, inputs: Vec<RecordId>, outputs: Vec<RecordId>) -> Transition {
        Transition {
            id: transition_id(hex_prefix),
            draft: TransitionDraft {
                transition_kind: TransitionKind::Shape,
                summary: "a transition".into(),
                rationale: None,
                inputs,
                outputs,
                created_at: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
                created_by: SessionId::new("s").unwrap(),
                import: None,
            },
        }
    }

    #[test]
    fn root_snapshot_members_are_exactly_the_root_members() {
        let root = vec![node("aa"), node("bb")];
        let state = members_at(&root, &[], SnapshotOrdinal::new(1).unwrap());
        assert_eq!(state.nodes(), root.as_slice());
        assert!(state.transitions().is_empty());
    }

    #[test]
    fn each_snapshot_adds_its_transition_and_its_outputs() {
        let root = vec![node("aa")];
        let output = node("bb");
        let transitions = vec![transition("11", vec![node("aa")], vec![output])];

        let state = members_at(&root, &transitions, SnapshotOrdinal::new(2).unwrap());
        assert_eq!(state.transitions(), &[transitions[0].id]);
        assert!(state.nodes().contains(&output));
        assert!(state.contains(&transitions[0].id));
    }

    #[test]
    fn inputs_are_not_added_again_they_are_already_members() {
        let input = node("aa");
        let root = vec![input];
        let transitions = vec![transition("11", vec![input], vec![node("bb")])];

        let state = members_at(&root, &transitions, SnapshotOrdinal::new(2).unwrap());
        assert_eq!(state.nodes().iter().filter(|id| **id == input).count(), 1);
    }

    #[test]
    fn membership_at_two_is_a_subset_of_membership_at_three() {
        let root = vec![node("aa")];
        let transitions = vec![
            transition("11", vec![node("aa")], vec![node("bb")]),
            transition("22", vec![node("bb")], vec![node("cc")]),
        ];

        let at_two = members_at(&root, &transitions, SnapshotOrdinal::new(2).unwrap());
        let at_three = members_at(&root, &transitions, SnapshotOrdinal::new(3).unwrap());

        for id in at_two.nodes() {
            assert!(at_three.nodes().contains(id));
        }
        for id in at_two.transitions() {
            assert!(at_three.transitions().contains(id));
        }
        assert!(at_three.nodes().contains(&node("cc")));
        assert!(!at_two.nodes().contains(&node("cc")));
    }
}
