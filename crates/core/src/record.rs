//! The immutable graph's records and the drafts that precede them.
//!
//! A draft is a record before it has an identity: hashing a draft through the
//! canonical encoder produces the [`crate::id::RecordId`] that names it.
//! "Parse, don't validate" — a draft is constructible only from parsed
//! values, and the only way to a record is through the encoder. An unencoded
//! record must be unspellable.

use crate::id::{Hash, InitiativeId, ProjectKey, RecordId, SessionId, SnapshotOrdinal};
use crate::kinds::{NodeKind, Relation, TransitionKind};
use crate::time::Timestamp;

/// A result node before it has an identity.
pub struct NodeDraft {
    pub node_kind: NodeKind,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub created_at: Timestamp,
    pub created_by: SessionId,
}

/// A transition before it has an identity.
pub struct TransitionDraft {
    pub transition_kind: TransitionKind,
    pub summary: String,
    pub rationale: Option<String>,
    /// Manifest order; the encoder does not sort these.
    pub inputs: Vec<RecordId>,
    /// Manifest order; the encoder does not sort these.
    pub outputs: Vec<RecordId>,
    pub created_at: Timestamp,
    pub created_by: SessionId,
    pub import: Option<ImportDraft>,
}

/// The import-specific fields of an import transition.
pub struct ImportDraft {
    pub source_initiative: InitiativeId,
    pub source_snapshot: SnapshotOrdinal,
    /// Sorted by the encoder, not by the caller.
    pub included: Vec<RecordId>,
    pub rationale: String,
}

/// A connection before it has an identity.
pub struct ConnectionDraft {
    pub transition: RecordId,
    pub from: RecordId,
    pub to: RecordId,
    pub relation: Relation,
    pub created_at: Timestamp,
}

/// An artifact's metadata before it has an identity.
pub struct ArtifactDraft {
    pub description: String,
    pub byte_size: u64,
    /// A pure SHA-256 of the artifact's bytes.
    pub content_hash: Hash,
    pub created_at: Timestamp,
    pub created_by: SessionId,
}

/// A stored result node: a draft plus the identity its encoding produced.
pub struct ResultNode {
    pub id: RecordId,
    pub draft: NodeDraft,
}

/// A stored transition: a draft plus the identity its encoding produced.
pub struct Transition {
    pub id: RecordId,
    pub draft: TransitionDraft,
}

/// A stored connection: a draft plus the identity its encoding produced.
pub struct Connection {
    pub id: RecordId,
    pub draft: ConnectionDraft,
}

/// A stored artifact's metadata: a draft plus the identity its encoding
/// produced.
pub struct ArtifactMeta {
    pub id: RecordId,
    pub draft: ArtifactDraft,
}

/// The complete graph state after an accepted transition, or the baseline at
/// the root.
pub struct Snapshot {
    pub initiative: InitiativeId,
    pub ordinal: SnapshotOrdinal,
    /// `None` only at a root snapshot.
    pub transition: Option<RecordId>,
    pub declared_base: Option<SnapshotOrdinal>,
    pub chain_hash: Hash,
    pub created_at: Timestamp,
}

/// The outer work-history container.
pub struct Initiative {
    pub id: InitiativeId,
    pub project: ProjectKey,
    pub name: String,
    pub destination: String,
    pub notes: Option<String>,
    pub created_at: Timestamp,
}
