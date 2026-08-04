//! Strict identifier values.
//!
//! Every identifier is a private-field newtype that can only be built by
//! parsing. Once a caller holds one, the value is known good: a numeric id is
//! positive, a project key is an absolute physical path, and a session id is a
//! short line of printable text.

pub mod hash;
pub mod numeric;
pub mod project;
pub mod record;

pub use hash::Hash;
pub use numeric::{DecisionId, InitiativeId, NoteId, QuestionId, RunAttachmentId, RunId, TicketId};
pub use project::{ProjectKey, SessionId};
pub use record::{RecordId, RecordKind, SnapshotOrdinal};
