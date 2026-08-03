//! Row and detail models shared by more than one document.

use crate::id::{AttachmentId, InitiativeId, SessionId, TicketId};
use crate::model::{PersistedInitiativeStatus, TicketStatusLabel, TicketType};
use crate::time::Timestamp;

/// The initiative facts every map-shaped document repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiativeHeader {
    /// The initiative's identifier.
    pub id: InitiativeId,
    /// Its name.
    pub name: String,
    /// Where the work is going.
    pub destination: String,
    /// Free-form notes, empty when there are none.
    pub notes: String,
    /// Whether the operator has closed it.
    pub status: PersistedInitiativeStatus,
}

/// One ticket that can be picked up right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
}

/// One settled ticket, as the map lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRow {
    /// The ticket the decision belongs to.
    pub ticket_id: TicketId,
    /// The ticket's title.
    pub title: String,
    /// The decision text, clamped when rendered.
    pub gist: String,
}

/// One attachment owned by the ticket being shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRow {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// Its file name.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// What it is for.
    pub description: String,
}

/// One attachment another ticket owns and this one points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedAttachmentRow {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// Its file name.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// The ticket that owns it.
    pub owner: TicketId,
    /// What it is for.
    pub description: String,
}

/// One ticket still waiting for a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
    /// Whether anyone holds it.
    pub status: TicketStatusLabel,
}

/// One decision, in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullDecision {
    /// The ticket the decision belongs to.
    pub ticket_id: TicketId,
    /// The ticket's title.
    pub title: String,
    /// The question that was asked.
    pub question: String,
    /// The decision text, never clamped.
    pub resolution: String,
}

/// One attachment, as the handoff table lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedAttachmentRow {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// The ticket that owns it.
    pub ticket_id: TicketId,
    /// Its file name.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// What it is for.
    pub description: String,
}

/// One row of the active session table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// The session's identifier.
    pub id: SessionId,
    /// The ticket it holds, with that ticket's title.
    pub holding: Option<(TicketId, String)>,
    /// When it was last heard from.
    pub last_seen_at: Timestamp,
}
