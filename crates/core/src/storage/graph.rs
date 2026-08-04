//! The immutable graph's storage capabilities.
//!
//! Every method takes and returns core types only — no connection, no
//! statement, no row, no SQL text. Later slices extend these traits; they
//! never fork a parallel one.

use crate::id::{InitiativeId, ProjectKey, RecordId};
use crate::outcome::graph::CreateInitiativeOutcome;
use crate::record::{Initiative, Snapshot};
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
