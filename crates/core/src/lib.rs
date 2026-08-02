//! Wayfind's functional core.
//!
//! Every business rule lives here as a pure function over strict values. The
//! crate performs no input or output: it never reads a clock, a file, an
//! environment variable, or a database. The shell in `wayfind_cli` supplies
//! every effect as data and applies every decision this crate returns.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod error;
pub mod id;
pub mod model;
pub mod time;

pub use error::{Error, Result};
pub use id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
pub use model::{
    ActiveSessionState, AttachmentMetadata, AttachmentReference, BlockedReason, Decision,
    Dependency, FogNote, FrontierTicket, Initiative, InitiativeState, NonEmptyVec, PersistedClaim,
    PersistedInitiativeStatus, PersistedSessionState, PersistedTicketState, ScopeExclusion,
    Session, SessionState, Ticket, TicketState, TicketType,
};
pub use time::Timestamp;
