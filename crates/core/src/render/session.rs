//! `wayfind session list`, `session resume`, and an unavailable `next`.

use std::fmt::Write as _;

use super::front_matter::FrontMatter;
use super::guidance::state_guidance;
use super::markdown::{cell, count};
use super::rows::SessionRow;
use crate::id::{InitiativeId, SessionId};
use crate::model::InitiativeState;

/// Everything `wayfind session list` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListView {
    /// The initiative the sessions are working.
    pub initiative_id: InitiativeId,
    /// Active sessions, newest activity first.
    pub sessions: Vec<SessionRow>,
}

/// Render the active session table.
pub fn render_session_list(model: &SessionListView) -> String {
    let mut out = FrontMatter::new("sessions")
        .number("initiative_id", model.initiative_id.get())
        .number("count", count(model.sessions.len()))
        .render();
    out.push_str("\n# Active Wayfind sessions\n\n");
    out.push_str(
        "| Session | State | Current ticket | Last activity |\n| --- | --- | --- | --- |\n",
    );
    for row in &model.sessions {
        let (state, ticket) = match &row.holding {
            Some((id, title)) => ("working", format!("[{}] {}", id, cell(title))),
            None => ("ready", "—".to_string()),
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            cell(row.id.as_str()),
            state,
            ticket,
            row.last_seen_at
        );
    }
    out
}

/// Everything `wayfind session resume` prints when the session holds nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResumeView {
    /// The session resuming.
    pub session_id: SessionId,
    /// The initiative it is bound to.
    pub initiative_id: InitiativeId,
    /// Where that initiative stands.
    pub state: InitiativeState,
}

/// Render a resumed session that is holding no ticket.
///
/// A session that holds a ticket gets that ticket printed instead; picking
/// between the two is the shell's job, because only the shell knows what was
/// read.
pub fn render_session_resume(model: &SessionResumeView) -> String {
    let mut out = FrontMatter::new("session")
        .text("id", model.session_id.as_str())
        .number("initiative_id", model.initiative_id.get())
        .text("status", model.state.as_str())
        .render();
    let _ = write!(
        out,
        "\n# Wayfind session\n\n{}\n",
        state_guidance(&model.state, model.initiative_id)
    );
    out
}

/// Everything `wayfind next` prints when nothing is available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextView {
    /// The initiative that has nothing to offer.
    pub initiative_id: InitiativeId,
    /// Why it has nothing to offer.
    pub state: InitiativeState,
}

/// Render `wayfind next` when the frontier is empty.
///
/// An empty frontier is an answer, not a failure, so this document is a success
/// and the command exits zero.
pub fn render_next_unavailable(model: &NextView) -> String {
    let mut out = FrontMatter::new("next")
        .number("initiative_id", model.initiative_id.get())
        .text("status", model.state.as_str())
        .render();
    let _ = write!(
        out,
        "\n# No available ticket\n\n{}\n",
        state_guidance(&model.state, model.initiative_id)
    );
    out
}
