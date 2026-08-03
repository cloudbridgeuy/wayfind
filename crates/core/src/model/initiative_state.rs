//! What an initiative's map says about the work as a whole.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::collections::NonEmptyVec;
use super::kinds::TicketType;
use crate::id::TicketId;

/// Why an initiative has open work but nothing available to pick up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedReason {
    /// Other sessions hold every ticket that would otherwise be available.
    ClaimsHoldFrontier {
        /// How many tickets are claimed. Always at least one.
        claimed: u32,
    },
    /// Every open ticket waits on a blocker that is not resolved yet.
    EveryOpenTicketIsBlocked,
}

/// One entry of the frontier: a ticket that can be picked up right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontierTicket {
    /// The ticket's identifier.
    pub id: TicketId,
    /// The ticket's title.
    pub title: String,
    /// The ticket's kind.
    pub ticket_type: TicketType,
}

/// What an initiative's map says about the work as a whole.
///
/// This is computed, never stored. [`PersistedInitiativeStatus`] is the stored
/// half of the pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum InitiativeState {
    /// No tickets yet; the map has not been charted.
    Charting,
    /// At least one ticket can be picked up right now.
    Ready {
        /// Every available ticket, ordered by identifier.
        frontier: NonEmptyVec<FrontierTicket>,
    },
    /// Work remains, but none of it is available.
    Blocked(BlockedReason),
    /// Every ticket has a decision, and the map has not been cleared yet.
    Complete,
    /// The operator closed the initiative with `initiative clear`.
    Clear,
}

impl InitiativeState {
    /// The single word the front matter reports for this state.
    pub fn as_str(&self) -> &'static str {
        match self {
            InitiativeState::Charting => "charting",
            InitiativeState::Ready { .. } => "ready",
            InitiativeState::Blocked(_) => "blocked",
            InitiativeState::Complete => "complete",
            InitiativeState::Clear => "clear",
        }
    }
}

impl fmt::Display for InitiativeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
