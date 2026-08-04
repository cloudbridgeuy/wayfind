//! Validating `CreateInitiativeCommand` into a fully hashed write.

use crate::encode::CanonicalBytes;
use crate::id::{Hash, ProjectKey, RecordId, SnapshotOrdinal};
use crate::record::NodeDraft;
use crate::time::Timestamp;

/// A draft plus the identity and bytes its encoding produced — ready for the
/// shell to write without recomputing anything.
pub struct Prepared<D> {
    pub id: RecordId,
    pub bytes: CanonicalBytes,
    pub draft: D,
}

/// The root snapshot a new initiative starts at.
pub struct PreparedRootSnapshot {
    pub ordinal: SnapshotOrdinal,
    pub chain_hash: Hash,
    pub created_at: Timestamp,
}

/// A `CreateInitiativeCommand` that has been checked and fully hashed.
///
/// The `_sealed` field is private, so this struct is constructible only from
/// within this module — an unvalidated write must be unspellable.
pub struct ValidatedInitiative {
    pub project: ProjectKey,
    pub name: String,
    pub destination: String,
    pub notes: Option<String>,
    pub destination_node: Prepared<NodeDraft>,
    pub snapshot: PreparedRootSnapshot,
    _sealed: (),
}
