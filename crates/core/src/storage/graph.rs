//! The immutable graph's storage capabilities.
//!
//! Every method takes and returns core types only — no connection, no
//! statement, no row, no SQL text. Later slices extend these traits; they
//! never fork a parallel one.

use crate::id::{Hash, InitiativeId, ProjectKey, RecordId, RecordKind, SnapshotOrdinal};
use crate::outcome::graph::CreateInitiativeOutcome;
use crate::record::{Initiative, ResultNode, Snapshot, Transition};
use crate::storage::values::StorageResult;
use crate::validate::initiative::ValidatedInitiative;

/// Reading the immutable graph.
pub trait GraphReader {
    /// One initiative.
    fn initiative(&self, id: InitiativeId) -> StorageResult<Option<Initiative>>;

    /// Every initiative of a project.
    fn initiatives(&self, project: &ProjectKey) -> StorageResult<Vec<Initiative>>;

    /// Every snapshot of an initiative, in ordinal order.
    fn snapshots(&self, id: InitiativeId) -> StorageResult<Vec<Snapshot>>;

    /// The root snapshot's membership.
    fn root_members(&self, id: InitiativeId) -> StorageResult<Vec<RecordId>>;

    /// One snapshot of an initiative.
    fn snapshot(
        &self,
        id: InitiativeId,
        ordinal: SnapshotOrdinal,
    ) -> StorageResult<Option<Snapshot>>;

    /// The transitions accepted through a snapshot, ordinal order, from
    /// snapshot 2 through `through`.
    fn accepted_transitions(
        &self,
        id: InitiativeId,
        through: SnapshotOrdinal,
    ) -> StorageResult<Vec<Transition>>;

    /// One result node.
    fn node(&self, hash: &Hash) -> StorageResult<Option<ResultNode>>;

    /// Every record of a kind whose hash starts with a hex prefix.
    ///
    /// Returns candidates, not an outcome — the core's `resolve` turns
    /// candidates into an outcome. The adapter must not decide ambiguity.
    fn resolve_prefix(&self, kind: RecordKind, hex: &str) -> StorageResult<Vec<RecordId>>;
}

/// Appending to the immutable graph.
pub trait GraphAppender {
    /// Write a validated initiative's destination node, root snapshot, and
    /// membership in one transaction.
    fn create_initiative(
        &self,
        validated: ValidatedInitiative,
    ) -> StorageResult<CreateInitiativeOutcome>;
}

/// Everything the immutable graph needs from a store.
pub trait GraphStorage: GraphReader + GraphAppender {}

impl<T> GraphStorage for T where T: GraphReader + GraphAppender {}

#[cfg(test)]
mod tests {
    use super::GraphStorage;

    /// Compiles only while [`GraphStorage`] stays object safe.
    fn accepts_any(_s: &dyn GraphStorage) {}

    #[test]
    fn graph_storage_is_object_safe() {
        let _: fn(&dyn GraphStorage) = accepts_any;
    }
}
