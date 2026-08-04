//! Outcomes of a mutation to the immutable graph.

use crate::id::InitiativeId;
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
