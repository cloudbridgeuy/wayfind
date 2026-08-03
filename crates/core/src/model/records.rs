//! The records that carry already-parsed values together.

use serde::{Deserialize, Deserializer, Serialize};

use super::initiative_state::FrontierTicket;
use super::kinds::{PersistedInitiativeStatus, TicketType};
use super::session_state::SessionState;
use super::ticket_state::TicketState;
use crate::error::{Error, Result};
use crate::id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
use crate::time::Timestamp;

/// One project: the directory Wayfind groups initiatives under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// The absolute physical path that names the project.
    pub key: ProjectKey,
    /// When the project first appeared.
    pub created_at: Timestamp,
}

/// One initiative: a destination and the notes that frame it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Initiative {
    /// The initiative's identifier.
    pub id: InitiativeId,
    /// The project the initiative belongs to.
    pub project_key: ProjectKey,
    /// The initiative's name, unique within the project.
    pub name: String,
    /// Where the work is going.
    pub destination: String,
    /// Free-form framing, possibly empty.
    pub notes: String,
    /// The stored status.
    pub status: PersistedInitiativeStatus,
    /// When the initiative was created.
    pub created_at: Timestamp,
}

/// One ticket: a question, and the decision that answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    /// The ticket's identifier.
    pub id: TicketId,
    /// The initiative the ticket belongs to.
    pub initiative_id: InitiativeId,
    /// The ticket's title, unique within the initiative.
    pub title: String,
    /// The ticket's kind.
    pub ticket_type: TicketType,
    /// The question the ticket asks.
    pub question: String,
    /// Where the ticket sits in its lifecycle.
    pub state: TicketState,
    /// When the ticket was created.
    pub created_at: Timestamp,
}

impl Ticket {
    /// The frontier entry for this ticket.
    pub fn to_frontier_entry(&self) -> FrontierTicket {
        FrontierTicket {
            id: self.id,
            title: self.title.clone(),
            ticket_type: self.ticket_type,
        }
    }
}

/// One edge of the dependency graph: `ticket_id` waits on `blocker_id`.
///
/// The fields are private because the pair carries an invariant the database
/// also enforces: a ticket cannot block itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Dependency {
    ticket_id: TicketId,
    blocker_id: TicketId,
}

impl Dependency {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "dependency";

    /// Parse a pair of identifiers into an edge, rejecting a self edge.
    pub fn new(ticket_id: TicketId, blocker_id: TicketId) -> Result<Self> {
        if ticket_id == blocker_id {
            return Err(Error::invalid_value(
                Self::FIELD,
                "a ticket cannot block itself",
            ));
        }
        Ok(Self {
            ticket_id,
            blocker_id,
        })
    }

    /// The ticket that waits.
    pub fn ticket_id(self) -> TicketId {
        self.ticket_id
    }

    /// The ticket that must resolve first.
    pub fn blocker_id(self) -> TicketId {
        self.blocker_id
    }
}

impl<'de> Deserialize<'de> for Dependency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            ticket_id: TicketId,
            blocker_id: TicketId,
        }
        let raw = Raw::deserialize(deserializer)?;
        Dependency::new(raw.ticket_id, raw.blocker_id).map_err(serde::de::Error::custom)
    }
}

/// One agent session working inside an initiative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The session's identifier, chosen by the agent runtime.
    pub id: SessionId,
    /// The project the session belongs to.
    pub project_key: ProjectKey,
    /// The initiative the session is bound to, if any.
    pub initiative_id: Option<InitiativeId>,
    /// Whether the session is active, and what it holds.
    pub state: SessionState,
    /// How many non-research tickets this session has already resolved.
    pub resolved_non_research_count: u32,
    /// When the session first appeared.
    pub started_at: Timestamp,
    /// When the session was last seen.
    pub last_seen_at: Timestamp,
}

impl Session {
    /// Whether this session has spent its one non-research resolution.
    pub fn has_spent_non_research_budget(&self) -> bool {
        self.resolved_non_research_count > 0
    }
}

/// One recorded decision, in the order decisions were made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The decision's identifier, which is also its order.
    pub id: DecisionId,
    /// The ticket the decision settles.
    pub ticket_id: TicketId,
    /// The first line of the resolution, kept for the index views.
    pub gist: String,
    /// When the decision was recorded.
    pub created_at: Timestamp,
}

/// One "not yet specified" note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FogNote {
    /// The note's identifier, which is also its order.
    pub id: NoteId,
    /// The initiative the note belongs to.
    pub initiative_id: InitiativeId,
    /// The note text.
    pub note: String,
    /// When the note was written.
    pub created_at: Timestamp,
}

/// One "out of scope" note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeExclusion {
    /// The exclusion's identifier, which is also its order.
    pub id: NoteId,
    /// The initiative the exclusion belongs to.
    pub initiative_id: InitiativeId,
    /// The exclusion text.
    pub note: String,
    /// When the exclusion was written.
    pub created_at: Timestamp,
}

/// Everything about an attachment except its bytes.
///
/// The bytes stay out of the index deliberately: listing attachments must not
/// load a megabyte per row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMetadata {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// The ticket that owns the attachment.
    pub ticket_id: TicketId,
    /// The attachment's file name, unique within its ticket.
    pub name: String,
    /// What the attachment holds.
    pub description: String,
    /// The raw byte count of the source document.
    pub byte_size: u64,
    /// The session that stored it, if it is known.
    pub session_id: Option<SessionId>,
    /// When the attachment was stored.
    pub created_at: Timestamp,
}

/// One reference from a ticket to an attachment another ticket owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentReference {
    /// The referenced attachment.
    pub attachment_id: AttachmentId,
    /// The ticket doing the referencing.
    pub ticket_id: TicketId,
    /// When the reference was made.
    pub created_at: Timestamp,
}
