//! Deciding whether a write is allowed, and building the command that performs
//! it.
//!
//! Every function here takes what the shell read and returns either a command or
//! the reason there is none. Nothing is written, nothing is read, and no clock is
//! consulted: the decision is a value, so it can be tested by writing down a
//! situation instead of by building a database.
//!
//! The checks run in the Bash script's order on purpose. When two rules are both
//! broken, the operator sees the same complaint the shell script gave, which is
//! what makes the port a port rather than a rewrite.
//!
//! Deciding here does not make the write safe on its own. Between the read and
//! the write, another agent may claim the same ticket. That is why each command
//! carries the revision it was decided at: the store checks it again, and refuses
//! with the same conflict vocabulary these functions use.

use crate::command::{
    AmendTicket, ClaimTicket, ClearInitiative, InsertDependency, ResolutionText, ResolveTicket,
};
use crate::graph::cycle_from;
use crate::id::{DecisionId, SessionId, TicketId};
use crate::initiative::InitiativeView;
use crate::model::{Dependency, TicketState};
use crate::outcome::{
    AmendConflict, ClaimConflict, ClearConflict, InsertDependencyConflict, ResolveConflict,
};
use crate::session::{session_of, SessionBudget};
use crate::time::Timestamp;

/// The result of deciding one write: a command, or the reason there is none.
pub type Decision<T, E> = std::result::Result<T, E>;

/// What the shell knows when a session asks for a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimInput {
    /// The ticket asked for.
    pub ticket_id: TicketId,
    /// The session asking.
    pub session_id: SessionId,
    /// The clock reading the shell took.
    pub now: Timestamp,
}

/// Decide whether a session may take a ticket, and build the command if it may.
///
/// A ticket absent from the view is reported as
/// [`ClaimConflict::TicketOutsideInitiative`], because that is what the operator
/// is being told: this ticket is not part of the map in play. Whether it exists
/// somewhere else is the store's business, not this decision's.
///
/// Taking a ticket the session already holds is allowed and is not a conflict.
/// `wayfind ticket claim` is idempotent for its own holder, so a repeated
/// command re-prints the ticket instead of complaining.
pub fn prepare_claim(
    input: ClaimInput,
    view: &InitiativeView,
) -> Decision<ClaimTicket, ClaimConflict> {
    let Some(ticket) = view.ticket(input.ticket_id) else {
        return Err(ClaimConflict::TicketOutsideInitiative {
            ticket_id: input.ticket_id,
        });
    };
    let budget = SessionBudget::of_optional(session_of(view, &input.session_id));

    if let Some(held) = budget.holding() {
        if held != ticket.id {
            return Err(ClaimConflict::SessionHoldsAnotherTicket { held });
        }
    }

    let claimant = match &ticket.state {
        TicketState::Open => None,
        TicketState::Claimed { claimant, .. } => Some(claimant.clone()),
        TicketState::Resolved { .. } | TicketState::Excluded => {
            return Err(ClaimConflict::TicketNotOpen {
                ticket_id: ticket.id,
                status: ticket.state.label(),
            })
        }
    };

    if !budget.may_take(ticket.ticket_type) {
        return Err(ClaimConflict::NonResearchBudgetSpent {
            ticket_id: ticket.id,
        });
    }

    if let Some(claimant) = &claimant {
        if claimant != &input.session_id {
            return Err(ClaimConflict::AlreadyClaimed {
                ticket_id: ticket.id,
                claimant: claimant.clone(),
            });
        }
    }

    Ok(ClaimTicket {
        ticket_id: ticket.id,
        initiative_id: view.id(),
        expected_initiative_revision: view.revision(),
        session_id: input.session_id,
        expected_session_holds: budget.holding(),
        expected_claimant: claimant,
        now: input.now,
    })
}

/// What the shell knows when a session settles a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveInput {
    /// The ticket being settled.
    pub ticket_id: TicketId,
    /// The session settling it.
    pub session_id: SessionId,
    /// The identifier already reserved for the new decision.
    pub decision_id: DecisionId,
    /// The decision text.
    pub resolution: ResolutionText,
    /// The clock reading the shell took.
    pub now: Timestamp,
}

/// Decide whether a session may settle a ticket, and build the command if it
/// may.
///
/// Only the holder of a claim may settle it. A ticket that already carries a
/// decision has no live claim, so it is reported as
/// [`ResolveConflict::NotClaimed`] — the same complaint the Bash script gives,
/// and the honest one: there is nothing here to settle.
pub fn prepare_resolution(
    input: ResolveInput,
    view: &InitiativeView,
) -> Decision<ResolveTicket, ResolveConflict> {
    let Some(ticket) = view.ticket(input.ticket_id) else {
        return Err(ResolveConflict::TicketOutsideInitiative {
            ticket_id: input.ticket_id,
        });
    };

    match &ticket.state {
        TicketState::Claimed { claimant, .. } if claimant == &input.session_id => {}
        TicketState::Claimed { claimant, .. } => {
            return Err(ResolveConflict::ClaimedByAnotherSession {
                ticket_id: ticket.id,
                claimant: claimant.clone(),
            })
        }
        TicketState::Open | TicketState::Resolved { .. } | TicketState::Excluded => {
            return Err(ResolveConflict::NotClaimed {
                ticket_id: ticket.id,
            })
        }
    }

    let budget = SessionBudget::of_optional(session_of(view, &input.session_id));
    if !budget.may_take(ticket.ticket_type) {
        return Err(ResolveConflict::NonResearchBudgetSpent {
            ticket_id: ticket.id,
        });
    }

    Ok(ResolveTicket {
        ticket_id: ticket.id,
        initiative_id: view.id(),
        expected_initiative_revision: view.revision(),
        session_id: input.session_id,
        decision_id: input.decision_id,
        ticket_type: ticket.ticket_type,
        resolution: input.resolution,
        now: input.now,
    })
}

/// What the shell knows when it repairs a recorded decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendInput {
    /// The ticket whose recorded text is wrong.
    pub ticket_id: TicketId,
    /// The corrected text.
    pub resolution: ResolutionText,
    /// The clock reading the shell took.
    pub now: Timestamp,
}

/// Decide whether a recorded decision may be repaired.
///
/// Amending needs no claim and no session: it corrects a transcription fault,
/// not a decision. It applies only to a ticket that already carries one.
pub fn prepare_amend(
    input: AmendInput,
    view: &InitiativeView,
) -> Decision<AmendTicket, AmendConflict> {
    let Some(ticket) = view.ticket(input.ticket_id) else {
        return Err(AmendConflict::TicketOutsideInitiative {
            ticket_id: input.ticket_id,
        });
    };
    if !matches!(ticket.state, TicketState::Resolved { .. }) {
        return Err(AmendConflict::NotResolved {
            ticket_id: ticket.id,
            status: ticket.state.label(),
        });
    }
    Ok(AmendTicket {
        ticket_id: ticket.id,
        initiative_id: view.id(),
        expected_initiative_revision: view.revision(),
        resolution: input.resolution,
        now: input.now,
    })
}

/// Decide whether an initiative may be closed.
///
/// A map closes when nothing is left to pick up or hand back. Tickets ruled out
/// of scope do not count as outstanding: dropping a question is a way of
/// finishing with it.
pub fn prepare_clear(
    view: &InitiativeView,
    now: Timestamp,
) -> Decision<ClearInitiative, ClearConflict> {
    let counts = view.counts();
    if counts.has_outstanding_work() {
        return Err(ClearConflict::OpenTicketsRemain {
            outstanding: counts.open + counts.claimed,
        });
    }
    Ok(ClearInitiative {
        initiative_id: view.id(),
        expected_initiative_revision: view.revision(),
        now,
    })
}

/// What the shell knows when it adds a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyInput {
    /// The ticket that would wait.
    pub ticket_id: TicketId,
    /// The ticket it would wait on.
    pub blocker_id: TicketId,
    /// The clock reading the shell took.
    pub now: Timestamp,
}

/// Decide whether an edge may be added, and build the command if it may.
///
/// An edge already in the graph is accepted rather than refused: adding it again
/// is a request for a state that already holds, and the store reports
/// `AlreadyPresent`.
pub fn prepare_dependency(
    input: DependencyInput,
    view: &InitiativeView,
) -> Decision<InsertDependency, InsertDependencyConflict> {
    let candidate = Dependency::new(input.ticket_id, input.blocker_id).map_err(|_| {
        InsertDependencyConflict::SelfEdge {
            ticket_id: input.ticket_id,
        }
    })?;
    for participant in [candidate.ticket_id(), candidate.blocker_id()] {
        if view.ticket(participant).is_none() {
            return Err(InsertDependencyConflict::TicketOutsideInitiative {
                ticket_id: participant,
            });
        }
    }
    if let Some(cycle) = cycle_from(view.dependencies(), candidate) {
        return Err(InsertDependencyConflict::WouldCloseCycle { cycle });
    }

    Ok(InsertDependency {
        ticket_id: input.ticket_id,
        blocker_id: input.blocker_id,
        initiative_id: view.id(),
        expected_initiative_revision: view.revision(),
        now: input.now,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{
        prepare_amend, prepare_claim, prepare_clear, prepare_dependency, prepare_resolution,
        AmendInput, ClaimInput, DependencyInput, ResolveInput,
    };
    use crate::command::ResolutionText;
    use crate::id::{DecisionId, InitiativeId, ProjectKey, SessionId, TicketId};
    use crate::initiative::InitiativeView;
    use crate::model::{
        ActiveSessionState, Dependency, Initiative, PersistedInitiativeStatus, Session,
        SessionState, Ticket, TicketState, TicketStatusLabel, TicketType,
    };
    use crate::outcome::{
        AmendConflict, ClaimConflict, ClearConflict, InsertDependencyConflict, ResolveConflict,
    };
    use crate::storage::InitiativeRevision;
    use crate::time::Timestamp;

    const REVISION: u64 = 7;

    fn moment() -> Timestamp {
        Timestamp::from_str("2026-08-02 13:45:09").unwrap()
    }

    fn ticket_id(value: i64) -> TicketId {
        TicketId::new(value).unwrap()
    }

    fn session_id(name: &str) -> SessionId {
        SessionId::new(name).unwrap()
    }

    fn ticket(value: i64, ticket_type: TicketType, state: TicketState) -> Ticket {
        Ticket {
            id: ticket_id(value),
            initiative_id: InitiativeId::new(1).unwrap(),
            title: format!("ticket {value}"),
            ticket_type,
            question: String::new(),
            state,
            created_at: moment(),
        }
    }

    fn resolved() -> TicketState {
        TicketState::Resolved {
            resolution: "settled".to_string(),
            resolved_at: moment(),
            amended_at: None,
        }
    }

    fn claimed_by(name: &str) -> TicketState {
        TicketState::Claimed {
            claimant: session_id(name),
            claimed_at: moment(),
        }
    }

    fn session(name: &str, state: SessionState, count: u32) -> Session {
        Session {
            id: session_id(name),
            project_key: ProjectKey::new("/Users/example/project").unwrap(),
            initiative_id: Some(InitiativeId::new(1).unwrap()),
            state,
            resolved_non_research_count: count,
            started_at: moment(),
            last_seen_at: moment(),
        }
    }

    fn view(
        tickets: Vec<Ticket>,
        edges: Vec<Dependency>,
        sessions: Vec<Session>,
    ) -> InitiativeView {
        InitiativeView::new(
            Initiative {
                id: InitiativeId::new(1).unwrap(),
                project_key: ProjectKey::new("/Users/example/project").unwrap(),
                name: "Cache the map".to_string(),
                destination: "A map that loads instantly".to_string(),
                notes: String::new(),
                status: PersistedInitiativeStatus::Working,
                created_at: moment(),
            },
            tickets,
            edges,
            sessions,
            InitiativeRevision::new(REVISION),
        )
        .unwrap()
    }

    fn claim(ticket: i64, session: &str) -> ClaimInput {
        ClaimInput {
            ticket_id: ticket_id(ticket),
            session_id: session_id(session),
            now: moment(),
        }
    }

    fn resolve(ticket: i64, session: &str) -> ResolveInput {
        ResolveInput {
            ticket_id: ticket_id(ticket),
            session_id: session_id(session),
            decision_id: DecisionId::new(1).unwrap(),
            resolution: ResolutionText::new("settled\nwith detail").unwrap(),
            now: moment(),
        }
    }

    fn edge(ticket: i64, blocker: i64) -> Dependency {
        Dependency::new(ticket_id(ticket), ticket_id(blocker)).unwrap()
    }

    // -- claiming ----------------------------------------------------------

    #[test]
    fn an_open_ticket_can_be_claimed_and_the_command_carries_the_revision_it_was_decided_at() {
        let view = view(
            vec![ticket(1, TicketType::Task, TicketState::Open)],
            Vec::new(),
            Vec::new(),
        );
        let command = prepare_claim(claim(1, "session-1"), &view).unwrap();
        assert_eq!(command.ticket_id, ticket_id(1));
        assert_eq!(command.expected_claimant, None);
        assert_eq!(command.expected_session_holds, None);
        assert_eq!(
            command.expected_initiative_revision,
            InitiativeRevision::new(REVISION)
        );
    }

    #[test]
    fn claiming_a_ticket_the_session_already_holds_is_allowed() {
        let view = view(
            vec![ticket(1, TicketType::Task, claimed_by("session-1"))],
            Vec::new(),
            vec![session(
                "session-1",
                SessionState::Active(ActiveSessionState::Holding {
                    ticket_id: ticket_id(1),
                }),
                0,
            )],
        );
        let command = prepare_claim(claim(1, "session-1"), &view).unwrap();
        assert_eq!(command.expected_claimant, Some(session_id("session-1")));
        assert_eq!(command.expected_session_holds, Some(ticket_id(1)));
    }

    #[test]
    fn a_ticket_another_session_holds_is_refused() {
        let view = view(
            vec![ticket(1, TicketType::Task, claimed_by("session-2"))],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_claim(claim(1, "session-1"), &view),
            Err(ClaimConflict::AlreadyClaimed {
                ticket_id: ticket_id(1),
                claimant: session_id("session-2"),
            })
        );
    }

    #[test]
    fn a_session_holding_one_ticket_cannot_take_another() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, claimed_by("session-1")),
                ticket(2, TicketType::Task, TicketState::Open),
            ],
            Vec::new(),
            vec![session(
                "session-1",
                SessionState::Active(ActiveSessionState::Holding {
                    ticket_id: ticket_id(1),
                }),
                0,
            )],
        );
        assert_eq!(
            prepare_claim(claim(2, "session-1"), &view),
            Err(ClaimConflict::SessionHoldsAnotherTicket { held: ticket_id(1) })
        );
    }

    #[test]
    fn a_settled_ticket_cannot_be_claimed() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, resolved()),
                ticket(2, TicketType::Task, TicketState::Excluded),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_claim(claim(1, "session-1"), &view),
            Err(ClaimConflict::TicketNotOpen {
                ticket_id: ticket_id(1),
                status: TicketStatusLabel::Resolved,
            })
        );
        assert_eq!(
            prepare_claim(claim(2, "session-1"), &view),
            Err(ClaimConflict::TicketNotOpen {
                ticket_id: ticket_id(2),
                status: TicketStatusLabel::Excluded,
            })
        );
    }

    #[test]
    fn a_ticket_of_another_initiative_is_not_on_this_map() {
        let view = view(Vec::new(), Vec::new(), Vec::new());
        assert_eq!(
            prepare_claim(claim(9, "session-1"), &view),
            Err(ClaimConflict::TicketOutsideInitiative {
                ticket_id: ticket_id(9)
            })
        );
    }

    #[test]
    fn a_spent_session_may_still_claim_research_but_nothing_else() {
        let spent = session(
            "session-1",
            SessionState::Active(ActiveSessionState::Ready),
            1,
        );
        let view = view(
            vec![
                ticket(1, TicketType::Task, TicketState::Open),
                ticket(2, TicketType::Research, TicketState::Open),
            ],
            Vec::new(),
            vec![spent],
        );
        assert_eq!(
            prepare_claim(claim(1, "session-1"), &view),
            Err(ClaimConflict::NonResearchBudgetSpent {
                ticket_id: ticket_id(1)
            })
        );
        assert!(prepare_claim(claim(2, "session-1"), &view).is_ok());
    }

    #[test]
    fn holding_another_ticket_is_reported_before_the_budget_is_checked() {
        let spent = session(
            "session-1",
            SessionState::Active(ActiveSessionState::Holding {
                ticket_id: ticket_id(1),
            }),
            1,
        );
        let view = view(
            vec![
                ticket(1, TicketType::Task, claimed_by("session-1")),
                ticket(2, TicketType::Task, TicketState::Open),
            ],
            Vec::new(),
            vec![spent],
        );
        assert_eq!(
            prepare_claim(claim(2, "session-1"), &view),
            Err(ClaimConflict::SessionHoldsAnotherTicket { held: ticket_id(1) })
        );
    }

    // -- resolving ---------------------------------------------------------

    #[test]
    fn the_holder_of_a_claim_may_settle_it() {
        let view = view(
            vec![ticket(1, TicketType::Task, claimed_by("session-1"))],
            Vec::new(),
            vec![session(
                "session-1",
                SessionState::Active(ActiveSessionState::Holding {
                    ticket_id: ticket_id(1),
                }),
                0,
            )],
        );
        let command = prepare_resolution(resolve(1, "session-1"), &view).unwrap();
        assert_eq!(command.decision_id, DecisionId::new(1).unwrap());
        assert_eq!(command.ticket_type, TicketType::Task);
        assert_eq!(command.resolution.gist(), "settled");
        assert!(command.spends_non_research_budget());
    }

    #[test]
    fn nobody_else_may_settle_a_claimed_ticket() {
        let view = view(
            vec![ticket(1, TicketType::Task, claimed_by("session-2"))],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_resolution(resolve(1, "session-1"), &view),
            Err(ResolveConflict::ClaimedByAnotherSession {
                ticket_id: ticket_id(1),
                claimant: session_id("session-2"),
            })
        );
    }

    #[test]
    fn an_unclaimed_or_already_settled_ticket_has_no_claim_to_settle() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, TicketState::Open),
                ticket(2, TicketType::Task, resolved()),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_resolution(resolve(1, "session-1"), &view),
            Err(ResolveConflict::NotClaimed {
                ticket_id: ticket_id(1)
            })
        );
        assert_eq!(
            prepare_resolution(resolve(2, "session-1"), &view),
            Err(ResolveConflict::NotClaimed {
                ticket_id: ticket_id(2)
            })
        );
    }

    #[test]
    fn the_non_research_limit_applies_at_resolution_as_well_as_at_claim() {
        let spent = session(
            "session-1",
            SessionState::Active(ActiveSessionState::Holding {
                ticket_id: ticket_id(1),
            }),
            1,
        );
        let view = view(
            vec![
                ticket(1, TicketType::Task, claimed_by("session-1")),
                ticket(2, TicketType::Research, claimed_by("session-1")),
            ],
            Vec::new(),
            vec![spent],
        );
        assert_eq!(
            prepare_resolution(resolve(1, "session-1"), &view),
            Err(ResolveConflict::NonResearchBudgetSpent {
                ticket_id: ticket_id(1)
            })
        );
        let command = prepare_resolution(resolve(2, "session-1"), &view).unwrap();
        assert!(!command.spends_non_research_budget());
    }

    // -- amending ----------------------------------------------------------

    fn amend(ticket: i64) -> AmendInput {
        AmendInput {
            ticket_id: ticket_id(ticket),
            resolution: ResolutionText::new("corrected").unwrap(),
            now: moment(),
        }
    }

    #[test]
    fn only_a_settled_ticket_can_have_its_text_repaired() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, resolved()),
                ticket(2, TicketType::Task, TicketState::Open),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert!(prepare_amend(amend(1), &view).is_ok());
        assert_eq!(
            prepare_amend(amend(2), &view),
            Err(AmendConflict::NotResolved {
                ticket_id: ticket_id(2),
                status: TicketStatusLabel::Open,
            })
        );
        assert_eq!(
            prepare_amend(amend(9), &view),
            Err(AmendConflict::TicketOutsideInitiative {
                ticket_id: ticket_id(9)
            })
        );
    }

    // -- clearing ----------------------------------------------------------

    #[test]
    fn a_map_closes_only_once_nothing_is_outstanding() {
        let settled = view(
            vec![
                ticket(1, TicketType::Task, resolved()),
                ticket(2, TicketType::Task, TicketState::Excluded),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert!(prepare_clear(&settled, moment()).is_ok());

        let busy = view(
            vec![
                ticket(1, TicketType::Task, TicketState::Open),
                ticket(2, TicketType::Task, claimed_by("session-1")),
            ],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_clear(&busy, moment()),
            Err(ClearConflict::OpenTicketsRemain { outstanding: 2 })
        );
    }

    // -- dependencies ------------------------------------------------------

    fn dependency(ticket: i64, blocker: i64) -> DependencyInput {
        DependencyInput {
            ticket_id: ticket_id(ticket),
            blocker_id: ticket_id(blocker),
            now: moment(),
        }
    }

    #[test]
    fn an_edge_between_two_tickets_of_this_map_is_accepted() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, TicketState::Open),
                ticket(2, TicketType::Task, TicketState::Open),
            ],
            Vec::new(),
            Vec::new(),
        );
        let command = prepare_dependency(dependency(2, 1), &view).unwrap();
        assert_eq!(command.ticket_id, ticket_id(2));
        assert_eq!(command.blocker_id, ticket_id(1));
        assert_eq!(
            command.expected_initiative_revision,
            InitiativeRevision::new(REVISION)
        );
    }

    #[test]
    fn a_ticket_cannot_be_made_to_wait_on_itself() {
        let view = view(
            vec![ticket(1, TicketType::Task, TicketState::Open)],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_dependency(dependency(1, 1), &view),
            Err(InsertDependencyConflict::SelfEdge {
                ticket_id: ticket_id(1)
            })
        );
    }

    #[test]
    fn an_edge_that_would_close_a_loop_is_refused_and_names_the_loop() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, TicketState::Open),
                ticket(2, TicketType::Task, TicketState::Open),
                ticket(3, TicketType::Task, TicketState::Open),
            ],
            vec![edge(2, 1), edge(3, 2)],
            Vec::new(),
        );
        assert_eq!(
            prepare_dependency(dependency(1, 3), &view),
            Err(InsertDependencyConflict::WouldCloseCycle {
                cycle: vec![ticket_id(1), ticket_id(3), ticket_id(2), ticket_id(1)],
            })
        );
    }

    #[test]
    fn an_edge_that_is_already_there_is_accepted_again() {
        let view = view(
            vec![
                ticket(1, TicketType::Task, TicketState::Open),
                ticket(2, TicketType::Task, TicketState::Open),
            ],
            vec![edge(2, 1)],
            Vec::new(),
        );
        assert!(prepare_dependency(dependency(2, 1), &view).is_ok());
    }

    #[test]
    fn an_edge_naming_a_ticket_off_this_map_is_refused() {
        let view = view(
            vec![ticket(1, TicketType::Task, TicketState::Open)],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            prepare_dependency(dependency(1, 9), &view),
            Err(InsertDependencyConflict::TicketOutsideInitiative {
                ticket_id: ticket_id(9)
            })
        );
        assert_eq!(
            prepare_dependency(dependency(9, 1), &view),
            Err(InsertDependencyConflict::TicketOutsideInitiative {
                ticket_id: ticket_id(9)
            })
        );
    }
}
