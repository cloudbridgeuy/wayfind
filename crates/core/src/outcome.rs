//! What a write actually did.
//!
//! Every write in Wayfind has more than two endings. A claim can succeed, or
//! find the ticket already held by this same session, or find it held by
//! another. A dependency can be inserted, or already be there. None of those are
//! failures of the store, and none of them are exceptional: they are the normal
//! result of two agents working the same map.
//!
//! So they are values. [`crate::storage::StorageError`] is kept for the three
//! things a caller cannot plan around — an unreachable store, a corrupt record,
//! a capacity limit — and everything else arrives as an enum the caller must
//! match on. A caller that forgets a case fails to compile rather than treating
//! "someone else has it" as success.
//!
//! Two commands share their conflict types with the core's own precondition
//! checks: [`ClaimConflict`] and [`ResolveConflict`] are what the pure
//! `prepare_claim` and `prepare_resolution` functions report, and also what the
//! store reports when the world moved between the check and the write. One
//! vocabulary, so the shell renders one message either way.

use serde::{Deserialize, Serialize};

use crate::id::{InitiativeId, ProjectKey, SessionId, TicketId};
use crate::model::{Session, TicketStatusLabel};
use crate::storage::InitiativeRevision;

// ---------------------------------------------------------------------------
// Shared conflict parts
// ---------------------------------------------------------------------------

/// The initiative moved between the read the shell decided on and the write.
///
/// This is always safe to retry: read again, decide again, submit again. It is
/// its own type because "retry me" is different advice from every other
/// conflict, all of which stay true no matter how often the caller retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleRevision {
    /// The revision the command expected.
    pub expected: InitiativeRevision,
    /// The revision the store actually holds.
    pub actual: InitiativeRevision,
}

// ---------------------------------------------------------------------------
// Claiming
// ---------------------------------------------------------------------------

/// Why a claim cannot be taken.
///
/// Shared with the core's `prepare_claim`, which reports the same cases from a
/// read-only view before any write is attempted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimConflict {
    /// There is no such ticket.
    NoSuchTicket {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The ticket belongs to a different initiative than the one in play.
    TicketOutsideInitiative {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The ticket is not available: it is resolved, or ruled out of scope.
    TicketNotOpen {
        /// The ticket that was asked for.
        ticket_id: TicketId,
        /// Where the ticket actually sits.
        status: TicketStatusLabel,
    },
    /// Another session holds the ticket.
    AlreadyClaimed {
        /// The ticket that was asked for.
        ticket_id: TicketId,
        /// The session that holds it.
        claimant: SessionId,
    },
    /// This session already holds a different ticket.
    SessionHoldsAnotherTicket {
        /// The ticket the session is holding.
        held: TicketId,
    },
    /// This session has already spent its one non-research resolution, so it may
    /// only take research tickets from here on.
    NonResearchBudgetSpent {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The initiative moved; read again and retry.
    StaleRevision(StaleRevision),
}

/// What a claim did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimOutcome {
    /// The session now holds the ticket, and did not before.
    Claimed,
    /// The session already held the ticket. Nothing changed, and nothing is
    /// wrong: `ticket claim` is idempotent for its own holder.
    AlreadyHeld,
    /// The claim was refused.
    Conflict(ClaimConflict),
}

// ---------------------------------------------------------------------------
// Resolving
// ---------------------------------------------------------------------------

/// Why a resolution cannot be recorded.
///
/// Shared with the core's `prepare_resolution`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolveConflict {
    /// There is no such ticket.
    NoSuchTicket {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The ticket belongs to a different initiative than the one in play.
    TicketOutsideInitiative {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// Nobody holds the ticket, so there is no claim to settle.
    NotClaimed {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// Another session holds the ticket. Only its holder may settle it.
    ClaimedByAnotherSession {
        /// The ticket that was asked for.
        ticket_id: TicketId,
        /// The session that holds it.
        claimant: SessionId,
    },
    /// The ticket already carries a decision.
    AlreadyResolved {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// This session has already spent its one non-research resolution.
    NonResearchBudgetSpent {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The initiative moved; read again and retry.
    StaleRevision(StaleRevision),
}

/// What a resolution did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolveOutcome {
    /// The decision is recorded and the session is free again.
    Resolved,
    /// The resolution was refused.
    Conflict(ResolveConflict),
}

// ---------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------

/// Why a dependency edge cannot be added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertDependencyConflict {
    /// One of the two tickets does not exist.
    NoSuchTicket {
        /// The ticket that is missing.
        ticket_id: TicketId,
    },
    /// The two tickets are not in the same initiative.
    TicketOutsideInitiative {
        /// The ticket that is elsewhere.
        ticket_id: TicketId,
    },
    /// The edge would make a ticket wait on itself directly.
    SelfEdge {
        /// The ticket named on both sides.
        ticket_id: TicketId,
    },
    /// The edge would make a ticket wait on itself through others.
    WouldCloseCycle {
        /// The path that would close, from the waiting ticket back to itself.
        cycle: Vec<TicketId>,
    },
    /// The initiative moved; read again and retry.
    StaleRevision(StaleRevision),
}

/// What adding a dependency did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertDependencyOutcome {
    /// The edge is new.
    Inserted,
    /// The edge was already there. Nothing changed, and nothing is wrong.
    AlreadyPresent,
    /// The edge was refused.
    Conflict(InsertDependencyConflict),
}

// ---------------------------------------------------------------------------
// Amending
// ---------------------------------------------------------------------------

/// Why a recorded decision cannot be repaired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendConflict {
    /// There is no such ticket.
    NoSuchTicket {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The ticket belongs to a different initiative than the one in play.
    TicketOutsideInitiative {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The ticket carries no decision, so there is no recorded text to repair.
    /// Amending never resolves a ticket.
    NotResolved {
        /// The ticket that was asked for.
        ticket_id: TicketId,
        /// Where the ticket actually sits.
        status: TicketStatusLabel,
    },
    /// The initiative moved; read again and retry.
    StaleRevision(StaleRevision),
}

/// What repairing a recorded decision did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendOutcome {
    /// The recorded text and its gist are corrected.
    Amended,
    /// The repair was refused.
    Conflict(AmendConflict),
}

// ---------------------------------------------------------------------------
// Clearing
// ---------------------------------------------------------------------------

/// Why an initiative cannot be closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearConflict {
    /// There is no such initiative.
    NoSuchInitiative,
    /// Work is still outstanding. An initiative closes only once every ticket
    /// carries a decision or is ruled out of scope.
    OpenTicketsRemain {
        /// How many tickets are still open or claimed.
        outstanding: u32,
    },
    /// The initiative moved; read again and retry.
    StaleRevision(StaleRevision),
}

/// What closing an initiative did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearOutcome {
    /// The initiative is closed.
    Cleared,
    /// The initiative was already closed. Nothing changed.
    AlreadyClear,
    /// The close was refused.
    Conflict(ClearConflict),
}

// ---------------------------------------------------------------------------
// Sessions and projects
// ---------------------------------------------------------------------------

/// Why a session cannot be recorded as alive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchSessionConflict {
    /// The session is already bound to a different project or a different
    /// initiative. A session never moves; a new place means a new session
    /// identifier.
    SessionBoundElsewhere {
        /// The project the session is bound to.
        owner_project: ProjectKey,
        /// The initiative the session is bound to, if it has one.
        owner_initiative: Option<InitiativeId>,
    },
}

/// What recording a session did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchSessionOutcome {
    /// The session is new, and this is its record.
    Started(Session),
    /// The session was already there, and this is its refreshed record.
    Refreshed(Session),
    /// The record was refused.
    Conflict(TouchSessionConflict),
}

impl TouchSessionOutcome {
    /// The session record, if the command was not refused.
    pub fn session(&self) -> Option<&Session> {
        match self {
            TouchSessionOutcome::Started(session) | TouchSessionOutcome::Refreshed(session) => {
                Some(session)
            }
            TouchSessionOutcome::Conflict(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

/// What deleting an attachment did.
///
/// Deleting is idempotent: asking twice is not an error, and the second answer
/// says so rather than pretending the first delete happened again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoveAttachmentOutcome {
    /// The attachment and every reference to it are gone.
    Removed {
        /// How many references were dropped with it.
        references_removed: u32,
    },
    /// There was no such attachment.
    NotFound,
}

/// Why a reference cannot be changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceConflict {
    /// There is no such attachment.
    NoSuchAttachment,
    /// There is no such ticket.
    NoSuchTicket {
        /// The ticket that was asked for.
        ticket_id: TicketId,
    },
    /// The ticket already owns the attachment, so a reference would say nothing.
    TicketOwnsAttachment {
        /// The ticket that owns it.
        ticket_id: TicketId,
    },
}

/// What changing a reference did.
///
/// Both adding and dropping a reference report through this type, because both
/// are idempotent and both have the same three ways to be refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceOutcome {
    /// The reference now exists, and did not before.
    Added,
    /// The reference was already there. Nothing changed.
    AlreadyPresent,
    /// The reference is gone, and existed before.
    Removed,
    /// There was no such reference to drop. Nothing changed.
    NotPresent,
    /// The change was refused.
    Conflict(ReferenceConflict),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{
        ClaimConflict, ClaimOutcome, ClearOutcome, InsertDependencyOutcome, ReferenceOutcome,
        RemoveAttachmentOutcome, StaleRevision, TouchSessionOutcome,
    };
    use crate::id::{ProjectKey, SessionId, TicketId};
    use crate::model::{ActiveSessionState, Session, SessionState};
    use crate::storage::InitiativeRevision;
    use crate::time::Timestamp;

    fn session() -> Session {
        let moment = Timestamp::from_str("2026-08-02 13:45:09").unwrap();
        Session {
            id: SessionId::new("session-1").unwrap(),
            project_key: ProjectKey::new("/Users/example/project").unwrap(),
            initiative_id: None,
            state: SessionState::Active(ActiveSessionState::Ready),
            resolved_non_research_count: 0,
            started_at: moment,
            last_seen_at: moment,
        }
    }

    #[test]
    fn an_idempotent_repeat_is_an_outcome_and_not_a_conflict() {
        assert!(!matches!(
            ClaimOutcome::AlreadyHeld,
            ClaimOutcome::Conflict(_)
        ));
        assert!(!matches!(
            InsertDependencyOutcome::AlreadyPresent,
            InsertDependencyOutcome::Conflict(_)
        ));
        assert!(!matches!(
            ClearOutcome::AlreadyClear,
            ClearOutcome::Conflict(_)
        ));
        assert!(!matches!(
            ReferenceOutcome::NotPresent,
            ReferenceOutcome::Conflict(_)
        ));
        assert!(!matches!(
            RemoveAttachmentOutcome::NotFound,
            RemoveAttachmentOutcome::Removed { .. }
        ));
    }

    #[test]
    fn a_stale_revision_reports_both_sides_so_the_shell_can_explain_the_retry() {
        let stale = StaleRevision {
            expected: InitiativeRevision::new(7),
            actual: InitiativeRevision::new(9),
        };
        let conflict = ClaimConflict::StaleRevision(stale);
        let ClaimConflict::StaleRevision(reported) = conflict else {
            panic!("expected a stale revision");
        };
        assert_eq!(reported.expected.get(), 7);
        assert_eq!(reported.actual.get(), 9);
    }

    #[test]
    fn touching_a_session_hands_back_its_record_either_way() {
        assert_eq!(
            TouchSessionOutcome::Started(session()).session(),
            Some(&session())
        );
        assert_eq!(
            TouchSessionOutcome::Refreshed(session()).session(),
            Some(&session())
        );
    }

    #[test]
    fn outcomes_round_trip_through_json() {
        let outcome = ClaimOutcome::Conflict(ClaimConflict::AlreadyClaimed {
            ticket_id: TicketId::new(3).unwrap(),
            claimant: SessionId::new("session-2").unwrap(),
        });
        let encoded = serde_json::to_string(&outcome).unwrap();
        assert_eq!(
            serde_json::from_str::<ClaimOutcome>(&encoded).unwrap(),
            outcome
        );
    }
}
