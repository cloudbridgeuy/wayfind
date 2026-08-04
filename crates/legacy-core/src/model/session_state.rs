//! What a session is, and the row it was read from.

use serde::{Deserialize, Serialize};

use super::corrupt;
use crate::error::{Error, Result};
use crate::id::TicketId;

/// Whether a session is still working, and what it holds if it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum SessionState {
    /// The session may still claim and resolve tickets.
    Active(ActiveSessionState),
    /// The session is finished and holds nothing.
    Closed,
}

/// What an active session is doing right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveSessionState {
    /// Free to take the next ticket.
    Ready,
    /// Holding one ticket, and therefore unable to take another.
    Holding {
        /// The ticket the session holds.
        ticket_id: TicketId,
    },
}

/// The stored columns that together describe one session's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistedSessionState<'a> {
    /// The value of `sessions.status`.
    pub status: &'a str,
    /// The value of `sessions.current_ticket_id`.
    pub current_ticket_id: Option<i64>,
}

impl SessionState {
    /// The entity name used when a stored session turns out to be impossible.
    const ENTITY: &'static str = "session";

    /// The exact text this state is stored as in `sessions.status`.
    pub fn as_status_str(&self) -> &'static str {
        match self {
            SessionState::Active(_) => "active",
            SessionState::Closed => "closed",
        }
    }

    /// The ticket this session holds, if it holds one.
    pub fn held_ticket(&self) -> Option<TicketId> {
        match self {
            SessionState::Active(ActiveSessionState::Holding { ticket_id }) => Some(*ticket_id),
            _ => None,
        }
    }

    /// Whether the session is still active.
    pub fn is_active(&self) -> bool {
        matches!(self, SessionState::Active(_))
    }

    /// Build a state from the stored columns, rejecting impossible combinations.
    pub fn from_persisted(persisted: PersistedSessionState<'_>) -> Result<Self> {
        match (persisted.status, persisted.current_ticket_id) {
            ("active", None) => Ok(SessionState::Active(ActiveSessionState::Ready)),
            ("active", Some(raw)) => Ok(SessionState::Active(ActiveSessionState::Holding {
                ticket_id: corrupt(TicketId::new(raw))?,
            })),
            ("closed", None) => Ok(SessionState::Closed),
            ("closed", Some(raw)) => Err(Error::corrupt_data(
                Self::ENTITY,
                format!("status is closed but the session still holds ticket {raw}"),
            )),
            (other, _) => Err(Error::corrupt_data(
                Self::ENTITY,
                format!("unknown status {other:?}"),
            )),
        }
    }
}
