//! The sentence that tells a reader what to do next.

use crate::id::InitiativeId;
use crate::model::{BlockedReason, InitiativeState};

/// What to do next, given where the initiative stands.
///
/// Every document that has nothing to show prints this instead of an error. An
/// empty frontier is an answer.
pub fn state_guidance(state: &InitiativeState, initiative: InitiativeId) -> String {
    match state {
        InitiativeState::Clear => format!(
            "Initiative {initiative} is clear. No frontier tickets remain.\n\
             Run `wayfind handoff` to collect the decisions, then hand off to `shaping` or `writing-plans`."
        ),
        InitiativeState::Complete => format!(
            "Initiative {initiative} has no open or claimed tickets left. \
             Run `wayfind initiative clear` to mark the map clear, then `wayfind handoff` to hand off."
        ),
        InitiativeState::Charting => format!(
            "Initiative {initiative} has no tickets yet. Chart the map with `wayfind ticket create`."
        ),
        InitiativeState::Blocked(BlockedReason::ClaimsHoldFrontier { claimed }) => format!(
            "No unblocked, unclaimed ticket is available; {claimed} claimed ticket(s) hold the frontier. \
             Run `wayfind session list`."
        ),
        InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked) => {
            "No unblocked, unclaimed ticket is available; every open ticket waits on an unresolved blocker. \
             Run `wayfind tree`."
                .to_string()
        }
        InitiativeState::Ready { .. } => "No active ticket is claimed. Run `wayfind next`.".to_string(),
    }
}
