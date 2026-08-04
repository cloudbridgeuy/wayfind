//! What a ticket is, and the row it was read from.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use super::corrupt;
use crate::error::{Error, Result};
use crate::id::SessionId;
use crate::time::Timestamp;

/// A ticket's lifecycle position, with the data each position implies.
///
/// A claimed ticket always names its claimant; a resolved ticket always carries
/// its resolution text. Neither fact needs checking downstream because neither
/// can be absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum TicketState {
    /// Nobody holds it and it has no decision yet.
    Open,
    /// One session holds it.
    Claimed {
        /// The session that holds the ticket.
        claimant: SessionId,
        /// When the claim was taken.
        claimed_at: Timestamp,
    },
    /// It carries a settled decision.
    Resolved {
        /// The full decision text.
        resolution: String,
        /// When the decision was recorded.
        resolved_at: Timestamp,
        /// When the recorded text was last repaired, if it ever was.
        amended_at: Option<Timestamp>,
    },
    /// Ruled out of the initiative without a decision.
    Excluded,
}

/// A live claim row, as the store holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistedClaim<'a> {
    /// The session named by `ticket_claims.session_id`.
    pub session_id: &'a str,
    /// The value of `ticket_claims.claimed_at`.
    pub claimed_at: &'a str,
}

/// The nullable columns that together describe one stored ticket's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PersistedTicketState<'a> {
    /// The value of `tickets.status`.
    pub status: &'a str,
    /// The value of `tickets.resolution`.
    pub resolution: Option<&'a str>,
    /// The value of `tickets.resolved_at`.
    pub resolved_at: Option<&'a str>,
    /// The value of `tickets.amended_at`.
    pub amended_at: Option<&'a str>,
    /// The matching `ticket_claims` row whose `released_at` is null, if any.
    pub live_claim: Option<PersistedClaim<'a>>,
}

/// A ticket's lifecycle position with the payload dropped.
///
/// The index views — the map frontier, the search results, the tree — print the
/// status word and nothing else. Carrying a label instead of a whole
/// [`TicketState`] keeps those reads from having to load a resolution or join a
/// claim just to print one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketStatusLabel {
    /// Nobody holds it and it has no decision yet.
    Open,
    /// One session holds it.
    Claimed,
    /// It carries a settled decision.
    Resolved,
    /// Ruled out of the initiative without a decision.
    Excluded,
}

impl TicketStatusLabel {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "ticket status";

    /// The exact text this label is stored as.
    pub fn as_str(self) -> &'static str {
        match self {
            TicketStatusLabel::Open => "open",
            TicketStatusLabel::Claimed => "claimed",
            TicketStatusLabel::Resolved => "resolved",
            TicketStatusLabel::Excluded => "excluded",
        }
    }

    /// Whether a ticket with this label still needs a decision.
    pub fn is_unresolved(self) -> bool {
        matches!(self, TicketStatusLabel::Open | TicketStatusLabel::Claimed)
    }
}

impl FromStr for TicketStatusLabel {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "open" => Ok(TicketStatusLabel::Open),
            "claimed" => Ok(TicketStatusLabel::Claimed),
            "resolved" => Ok(TicketStatusLabel::Resolved),
            "excluded" => Ok(TicketStatusLabel::Excluded),
            other => Err(Error::invalid_value(
                Self::FIELD,
                format!("expected one of open, claimed, resolved, excluded; got {other:?}"),
            )),
        }
    }
}

impl fmt::Display for TicketStatusLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TicketStatusLabel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl TicketState {
    /// The entity name used when a stored ticket turns out to be impossible.
    const ENTITY: &'static str = "ticket";

    /// This state with its payload dropped.
    pub fn label(&self) -> TicketStatusLabel {
        match self {
            TicketState::Open => TicketStatusLabel::Open,
            TicketState::Claimed { .. } => TicketStatusLabel::Claimed,
            TicketState::Resolved { .. } => TicketStatusLabel::Resolved,
            TicketState::Excluded => TicketStatusLabel::Excluded,
        }
    }

    /// The exact text this state is stored as in `tickets.status`.
    pub fn as_status_str(&self) -> &'static str {
        self.label().as_str()
    }

    /// Whether this ticket still needs a decision.
    ///
    /// This is the `status IN ('open', 'claimed')` predicate the Bash script
    /// uses to count unresolved work.
    pub fn is_unresolved(&self) -> bool {
        matches!(self, TicketState::Open | TicketState::Claimed { .. })
    }

    /// Whether a blocker in this state stops the ticket it blocks.
    pub fn blocks_dependents(&self) -> bool {
        !matches!(self, TicketState::Resolved { .. })
    }

    /// The session holding this ticket, if one does.
    pub fn claimant(&self) -> Option<&SessionId> {
        match self {
            TicketState::Claimed { claimant, .. } => Some(claimant),
            _ => None,
        }
    }

    /// The settled decision text, if there is one.
    pub fn resolution(&self) -> Option<&str> {
        match self {
            TicketState::Resolved { resolution, .. } => Some(resolution),
            _ => None,
        }
    }

    /// Build a state from the stored columns, rejecting impossible combinations.
    pub fn from_persisted(persisted: PersistedTicketState<'_>) -> Result<Self> {
        match persisted.status {
            "open" => {
                Self::require_no_decision(&persisted, "open")?;
                Self::require_no_live_claim(&persisted, "open")?;
                Ok(TicketState::Open)
            }
            "claimed" => {
                Self::require_no_decision(&persisted, "claimed")?;
                let claim = persisted.live_claim.ok_or_else(|| {
                    Error::corrupt_data(
                        Self::ENTITY,
                        "status is claimed but no unreleased claim exists",
                    )
                })?;
                Ok(TicketState::Claimed {
                    claimant: corrupt(SessionId::new(claim.session_id))?,
                    claimed_at: corrupt(claim.claimed_at.parse())?,
                })
            }
            "resolved" => {
                Self::require_no_live_claim(&persisted, "resolved")?;
                let resolution = persisted.resolution.filter(|text| !text.is_empty());
                let resolution = resolution.ok_or_else(|| {
                    Error::corrupt_data(Self::ENTITY, "status is resolved but resolution is empty")
                })?;
                let resolved_at = persisted.resolved_at.ok_or_else(|| {
                    Error::corrupt_data(Self::ENTITY, "status is resolved but resolved_at is null")
                })?;
                Ok(TicketState::Resolved {
                    resolution: resolution.to_owned(),
                    resolved_at: corrupt(resolved_at.parse())?,
                    amended_at: persisted
                        .amended_at
                        .filter(|text| !text.is_empty())
                        .map(|text| corrupt(text.parse()))
                        .transpose()?,
                })
            }
            "excluded" => {
                Self::require_no_decision(&persisted, "excluded")?;
                Self::require_no_live_claim(&persisted, "excluded")?;
                Ok(TicketState::Excluded)
            }
            other => Err(Error::corrupt_data(
                Self::ENTITY,
                format!("unknown status {other:?}"),
            )),
        }
    }

    /// Reject decision columns on a ticket that has no decision.
    fn require_no_decision(persisted: &PersistedTicketState<'_>, status: &str) -> Result<()> {
        if persisted.resolved_at.is_some_and(|text| !text.is_empty()) {
            return Err(Error::corrupt_data(
                Self::ENTITY,
                format!("status is {status} but resolved_at is set"),
            ));
        }
        if persisted.amended_at.is_some_and(|text| !text.is_empty()) {
            return Err(Error::corrupt_data(
                Self::ENTITY,
                format!("status is {status} but amended_at is set"),
            ));
        }
        if persisted.resolution.is_some_and(|text| !text.is_empty()) {
            return Err(Error::corrupt_data(
                Self::ENTITY,
                format!("status is {status} but a resolution is stored"),
            ));
        }
        Ok(())
    }

    /// Reject a live claim on a ticket that nobody may be holding.
    fn require_no_live_claim(persisted: &PersistedTicketState<'_>, status: &str) -> Result<()> {
        if persisted.live_claim.is_some() {
            return Err(Error::corrupt_data(
                Self::ENTITY,
                format!("status is {status} but an unreleased claim exists"),
            ));
        }
        Ok(())
    }
}
