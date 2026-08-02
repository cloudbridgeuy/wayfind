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

use std::fmt;
use std::num::NonZeroUsize;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};
use crate::id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
use crate::time::Timestamp;

// ---------------------------------------------------------------------------
// NonEmptyVec
// ---------------------------------------------------------------------------

/// A vector that is known to hold at least one element.
///
/// This is what lets [`InitiativeState::Ready`] mean "ready": a ready
/// initiative has a frontier, and a frontier with nothing in it cannot be
/// built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "non-empty list";

    /// The first element, which always exists.
    pub fn first(&self) -> &T {
        // `self.0` is non-empty by construction, so indexing is total here.
        &self.0[0]
    }

    /// A borrowed view of every element.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Iterate over the elements.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// How many elements there are, which is never zero.
    pub fn count(&self) -> NonZeroUsize {
        NonZeroUsize::new(self.0.len()).unwrap_or(NonZeroUsize::MIN)
    }

    /// Consume the wrapper and return the plain vector.
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T> TryFrom<Vec<T>> for NonEmptyVec<T> {
    type Error = Error;

    fn try_from(values: Vec<T>) -> Result<Self> {
        if values.is_empty() {
            return Err(Error::invalid_value(
                Self::FIELD,
                "must hold at least one element",
            ));
        }
        Ok(Self(values))
    }
}

impl<T> IntoIterator for NonEmptyVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a NonEmptyVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyVec<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let values = Vec::<T>::deserialize(deserializer)?;
        Self::try_from(values).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Closed enums
// ---------------------------------------------------------------------------

/// The four kinds of ticket, matching the `tickets.type` check constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketType {
    /// A question to grill an assumption with.
    Grilling,
    /// An investigation that gathers facts without settling design.
    Research,
    /// A build-to-learn spike.
    Prototype,
    /// Ordinary work with a definite finish.
    Task,
}

impl TicketType {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "ticket type";

    /// Every variant, in the order the check constraint lists them.
    pub const ALL: [TicketType; 4] = [
        TicketType::Grilling,
        TicketType::Research,
        TicketType::Prototype,
        TicketType::Task,
    ];

    /// The exact text this variant is stored as.
    pub fn as_str(self) -> &'static str {
        match self {
            TicketType::Grilling => "grilling",
            TicketType::Research => "research",
            TicketType::Prototype => "prototype",
            TicketType::Task => "task",
        }
    }

    /// Whether resolving this ticket leaves a session free to take another.
    ///
    /// A session may resolve any number of research tickets, but only one
    /// non-research ticket. This is the predicate that rule reads.
    pub fn is_research(self) -> bool {
        matches!(self, TicketType::Research)
    }
}

impl FromStr for TicketType {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "grilling" => Ok(TicketType::Grilling),
            "research" => Ok(TicketType::Research),
            "prototype" => Ok(TicketType::Prototype),
            "task" => Ok(TicketType::Task),
            other => Err(Error::invalid_value(
                Self::FIELD,
                format!("expected one of grilling, research, prototype, task; got {other:?}"),
            )),
        }
    }
}

impl fmt::Display for TicketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TicketType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The status column of an initiative, exactly as the database stores it.
///
/// This is not the same as [`InitiativeState`]. The stored status records a
/// deliberate operator action; the state is computed from the status *and* the
/// ticket graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistedInitiativeStatus {
    /// Created, no work committed yet.
    Charting,
    /// Work is under way.
    Working,
    /// Deliberately closed by `initiative clear`.
    Clear,
}

impl PersistedInitiativeStatus {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "initiative status";

    /// The exact text this variant is stored as.
    pub fn as_str(self) -> &'static str {
        match self {
            PersistedInitiativeStatus::Charting => "charting",
            PersistedInitiativeStatus::Working => "working",
            PersistedInitiativeStatus::Clear => "clear",
        }
    }

    /// Whether an implicit write should refuse to touch this initiative.
    pub fn is_clear(self) -> bool {
        matches!(self, PersistedInitiativeStatus::Clear)
    }
}

impl FromStr for PersistedInitiativeStatus {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "charting" => Ok(PersistedInitiativeStatus::Charting),
            "working" => Ok(PersistedInitiativeStatus::Working),
            "clear" => Ok(PersistedInitiativeStatus::Clear),
            other => Err(Error::invalid_value(
                Self::FIELD,
                format!("expected one of charting, working, clear; got {other:?}"),
            )),
        }
    }
}

impl fmt::Display for PersistedInitiativeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PersistedInitiativeStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Ticket state
// ---------------------------------------------------------------------------

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

impl TicketState {
    /// The entity name used when a stored ticket turns out to be impossible.
    const ENTITY: &'static str = "ticket";

    /// The exact text this state is stored as in `tickets.status`.
    pub fn as_status_str(&self) -> &'static str {
        match self {
            TicketState::Open => "open",
            TicketState::Claimed { .. } => "claimed",
            TicketState::Resolved { .. } => "resolved",
            TicketState::Excluded => "excluded",
        }
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

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

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

/// Re-label a parse failure as corrupt stored data.
///
/// A bad value read out of the database is not the caller's fault, so it must
/// not surface as [`Error::InvalidValue`].
fn corrupt<T>(parsed: Result<T>) -> Result<T> {
    parsed.map_err(|error| Error::corrupt_data("record", error.to_string()))
}

// ---------------------------------------------------------------------------
// Initiative state
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        ActiveSessionState, Dependency, InitiativeState, NonEmptyVec, PersistedClaim,
        PersistedInitiativeStatus, PersistedSessionState, PersistedTicketState, SessionState,
        TicketState, TicketType,
    };
    use crate::error::Error;
    use crate::id::{SessionId, TicketId};

    fn ticket_id(value: i64) -> TicketId {
        TicketId::new(value).unwrap()
    }

    #[test]
    fn ticket_types_round_trip_through_their_stored_text() {
        for kind in TicketType::ALL {
            assert_eq!(kind.as_str().parse::<TicketType>().unwrap(), kind);
        }
        assert_eq!(TicketType::Research.to_string(), "research");
    }

    #[test]
    fn ticket_types_reject_anything_outside_the_check_constraint() {
        assert!("bug".parse::<TicketType>().is_err());
        assert!("Research".parse::<TicketType>().is_err());
        assert!("".parse::<TicketType>().is_err());
    }

    #[test]
    fn only_research_tickets_are_research() {
        assert!(TicketType::Research.is_research());
        assert!(!TicketType::Task.is_research());
        assert!(!TicketType::Grilling.is_research());
        assert!(!TicketType::Prototype.is_research());
    }

    #[test]
    fn initiative_statuses_round_trip_through_their_stored_text() {
        for status in [
            PersistedInitiativeStatus::Charting,
            PersistedInitiativeStatus::Working,
            PersistedInitiativeStatus::Clear,
        ] {
            assert_eq!(
                status
                    .as_str()
                    .parse::<PersistedInitiativeStatus>()
                    .unwrap(),
                status
            );
        }
        assert!("cleared".parse::<PersistedInitiativeStatus>().is_err());
        assert!(PersistedInitiativeStatus::Clear.is_clear());
        assert!(!PersistedInitiativeStatus::Working.is_clear());
    }

    #[test]
    fn non_empty_vec_rejects_an_empty_vector() {
        assert!(matches!(
            NonEmptyVec::<u8>::try_from(Vec::new()),
            Err(Error::InvalidValue { .. })
        ));
    }

    #[test]
    fn non_empty_vec_exposes_a_first_element_and_a_non_zero_count() {
        let values = NonEmptyVec::try_from(vec![3, 1, 2]).unwrap();
        assert_eq!(*values.first(), 3);
        assert_eq!(values.count().get(), 3);
        assert_eq!(values.as_slice(), &[3, 1, 2]);
        assert_eq!(values.into_vec(), vec![3, 1, 2]);
    }

    #[test]
    fn an_open_ticket_parses_from_a_bare_row() {
        let state = TicketState::from_persisted(PersistedTicketState {
            status: "open",
            ..PersistedTicketState::default()
        })
        .unwrap();
        assert_eq!(state, TicketState::Open);
        assert!(state.is_unresolved());
        assert!(state.blocks_dependents());
    }

    #[test]
    fn a_claimed_ticket_carries_its_claimant() {
        let state = TicketState::from_persisted(PersistedTicketState {
            status: "claimed",
            live_claim: Some(PersistedClaim {
                session_id: "session-1",
                claimed_at: "2026-08-02 10:00:00",
            }),
            ..PersistedTicketState::default()
        })
        .unwrap();
        assert_eq!(
            state.claimant(),
            Some(&SessionId::new("session-1").unwrap())
        );
        assert!(state.is_unresolved());
    }

    #[test]
    fn a_resolved_ticket_carries_its_decision() {
        let state = TicketState::from_persisted(PersistedTicketState {
            status: "resolved",
            resolution: Some("We use SQLite."),
            resolved_at: Some("2026-08-02 11:00:00"),
            ..PersistedTicketState::default()
        })
        .unwrap();
        assert_eq!(state.resolution(), Some("We use SQLite."));
        assert!(!state.is_unresolved());
        assert!(!state.blocks_dependents());
    }

    #[test]
    fn a_resolved_ticket_may_record_an_amendment() {
        let state = TicketState::from_persisted(PersistedTicketState {
            status: "resolved",
            resolution: Some("We use SQLite."),
            resolved_at: Some("2026-08-02 11:00:00"),
            amended_at: Some("2026-08-02 12:00:00"),
            ..PersistedTicketState::default()
        })
        .unwrap();
        assert!(matches!(
            state,
            TicketState::Resolved {
                amended_at: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn a_claimed_ticket_without_a_live_claim_is_corrupt() {
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "claimed",
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
    }

    #[test]
    fn an_open_ticket_with_a_live_claim_is_corrupt() {
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "open",
                live_claim: Some(PersistedClaim {
                    session_id: "session-1",
                    claimed_at: "2026-08-02 10:00:00",
                }),
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
    }

    #[test]
    fn a_resolved_ticket_without_a_resolution_is_corrupt() {
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "resolved",
                resolved_at: Some("2026-08-02 11:00:00"),
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "resolved",
                resolution: Some(""),
                resolved_at: Some("2026-08-02 11:00:00"),
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
    }

    #[test]
    fn a_resolved_ticket_without_a_resolved_at_is_corrupt() {
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "resolved",
                resolution: Some("We use SQLite."),
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
    }

    #[test]
    fn an_open_ticket_that_carries_decision_columns_is_corrupt() {
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "open",
                resolution: Some("leaked"),
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "open",
                amended_at: Some("2026-08-02 12:00:00"),
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
    }

    #[test]
    fn an_unknown_ticket_status_is_corrupt() {
        assert!(matches!(
            TicketState::from_persisted(PersistedTicketState {
                status: "in-progress",
                ..PersistedTicketState::default()
            }),
            Err(Error::CorruptData {
                entity: "ticket",
                ..
            })
        ));
    }

    #[test]
    fn an_excluded_ticket_parses_and_still_blocks_dependents() {
        let state = TicketState::from_persisted(PersistedTicketState {
            status: "excluded",
            ..PersistedTicketState::default()
        })
        .unwrap();
        assert_eq!(state, TicketState::Excluded);
        assert!(!state.is_unresolved());
        assert!(state.blocks_dependents());
    }

    #[test]
    fn ticket_states_report_their_stored_status_text() {
        assert_eq!(TicketState::Open.as_status_str(), "open");
        assert_eq!(TicketState::Excluded.as_status_str(), "excluded");
    }

    #[test]
    fn an_active_session_is_ready_or_holding() {
        assert_eq!(
            SessionState::from_persisted(PersistedSessionState {
                status: "active",
                current_ticket_id: None,
            })
            .unwrap(),
            SessionState::Active(ActiveSessionState::Ready)
        );
        assert_eq!(
            SessionState::from_persisted(PersistedSessionState {
                status: "active",
                current_ticket_id: Some(4),
            })
            .unwrap()
            .held_ticket(),
            Some(ticket_id(4))
        );
    }

    #[test]
    fn a_closed_session_that_still_holds_a_ticket_is_corrupt() {
        assert!(matches!(
            SessionState::from_persisted(PersistedSessionState {
                status: "closed",
                current_ticket_id: Some(4),
            }),
            Err(Error::CorruptData {
                entity: "session",
                ..
            })
        ));
    }

    #[test]
    fn an_unknown_session_status_is_corrupt() {
        assert!(matches!(
            SessionState::from_persisted(PersistedSessionState {
                status: "paused",
                current_ticket_id: None,
            }),
            Err(Error::CorruptData {
                entity: "session",
                ..
            })
        ));
    }

    #[test]
    fn a_session_holding_a_non_positive_ticket_id_is_corrupt() {
        assert!(matches!(
            SessionState::from_persisted(PersistedSessionState {
                status: "active",
                current_ticket_id: Some(0),
            }),
            Err(Error::CorruptData { .. })
        ));
    }

    #[test]
    fn dependencies_reject_a_self_edge() {
        assert!(Dependency::new(ticket_id(1), ticket_id(2)).is_ok());
        assert!(matches!(
            Dependency::new(ticket_id(1), ticket_id(1)),
            Err(Error::InvalidValue {
                field: "dependency",
                ..
            })
        ));
    }

    #[test]
    fn dependency_accessors_keep_the_direction_straight() {
        let edge = Dependency::new(ticket_id(5), ticket_id(3)).unwrap();
        assert_eq!(edge.ticket_id(), ticket_id(5));
        assert_eq!(edge.blocker_id(), ticket_id(3));
    }

    #[test]
    fn initiative_states_report_one_word_each() {
        assert_eq!(InitiativeState::Charting.as_str(), "charting");
        assert_eq!(InitiativeState::Complete.as_str(), "complete");
        assert_eq!(InitiativeState::Clear.to_string(), "clear");
    }
}
