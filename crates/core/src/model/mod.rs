//! The domain's closed value set.
//!
//! Two kinds of type live here. Closed enums such as [`TicketType`] and
//! [`TicketState`] describe what a thing *is*; a variant that the domain
//! forbids simply has no representation. Records such as [`Ticket`] carry those
//! values together and are built only from already-parsed parts.
//!
//! The persisted forms are deliberately separate. A stored ticket is a row with
//! nullable columns, and many of its column combinations are impossible. The
//! `from_persisted` constructors are where that row becomes a value, and where
//! an impossible combination becomes [`Error::CorruptData`].

use crate::error::{Error, Result};

mod collections;
mod initiative_state;
mod kinds;
mod records;
mod session_state;
mod ticket_state;

#[cfg(test)]
mod tests;

pub use collections::NonEmptyVec;
pub use initiative_state::{BlockedReason, FrontierTicket, InitiativeState};
pub use kinds::{PersistedInitiativeStatus, TicketType};
pub use records::{
    AttachmentMetadata, AttachmentReference, Decision, Dependency, FogNote, Initiative, Project,
    ScopeExclusion, Session, Ticket,
};
pub use session_state::{ActiveSessionState, PersistedSessionState, SessionState};
pub use ticket_state::{PersistedClaim, PersistedTicketState, TicketState, TicketStatusLabel};

/// Re-label a parse failure as corrupt stored data.
///
/// A bad value read out of the database is not the caller's fault, so it must
/// not surface as [`Error::InvalidValue`].
fn corrupt<T>(parsed: Result<T>) -> Result<T> {
    parsed.map_err(|error| Error::corrupt_data("record", error.to_string()))
}
