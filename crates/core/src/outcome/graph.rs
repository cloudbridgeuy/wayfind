//! Outcomes of a mutation to, or a search across, the immutable graph.

use crate::id::{InitiativeId, RecordId};
use crate::record::Initiative;

/// What creating an initiative produced.
pub enum CreateInitiativeOutcome {
    /// The initiative was created.
    Created(Initiative),
    /// Another initiative in the same project already holds that name.
    NameTaken {
        /// The initiative that already holds the name.
        existing: InitiativeId,
    },
}

/// What resolving an identifier prefix against a store's candidates found.
///
/// Ambiguity is a fact about the store, not a failure of the caller: the
/// renderer decides the exit code, this outcome only reports what matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveOutcome {
    /// Exactly one candidate matched.
    Unique(RecordId),
    /// More than one candidate matched, sorted by hex.
    Ambiguous(Vec<RecordId>),
    /// No candidate matched.
    Unknown,
}
