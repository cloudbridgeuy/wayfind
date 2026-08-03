//! The ticket lifecycle.
//!
//! A ticket is one question the initiative has to answer. It is created open,
//! taken by exactly one session, settled with a decision, and from then on it
//! only ever has its recorded text repaired. Every one of those steps is
//! decided by `wayfind_core` from a view of the whole initiative; this module
//! reads the view, hands it over, and applies whatever comes back.

use wayfind_core::{
    next_ticket, prepare_amend, prepare_claim, prepare_dependency, prepare_resolution,
    render_next_unavailable, AmendInput, AmendOutcome, ClaimInput, ClaimOutcome, CreateTicket,
    DependencyInput, IdScope, InitiativeId, InsertDependencyOutcome, NextView, NonEmptyText,
    ResolveInput, ResolveOutcome, TicketId, TicketType,
};

use super::{amend_refusal, claim_refusal, dependency_refusal, resolve_refusal, Shell};
use crate::args::TextSource;
use crate::error::ShellResult;
use crate::input;
use crate::output::Output;

/// Show one ticket of the initiative in play.
pub fn show(shell: &Shell<'_>, id: i64, output: &mut dyn Output) -> ShellResult<()> {
    let ticket_id = TicketId::new(id)?;
    let view = shell.readable_view()?;
    let document = shell.ticket_document(&view, ticket_id)?;
    output.text(&document)
}

/// Show the ticket a session should pick up next.
///
/// An initiative with nothing available is an answer, not a failure: the
/// document says why there is nothing and what to do about it, and the command
/// succeeds.
pub fn next(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.readable_view()?;
    let state = wayfind_core::classify_initiative(&view);
    match next_ticket(&state) {
        Some(entry) => {
            let document = shell.ticket_document(&view, entry.id)?;
            output.text(&document)
        }
        None => output.text(&render_next_unavailable(&NextView {
            initiative_id: view.id(),
            state,
        })),
    }
}

/// Add a question to the initiative in play.
pub fn create(
    shell: &Shell<'_>,
    title: &str,
    ticket_type: TicketType,
    question: &str,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let view = shell.writable_view()?;
    let id = shell.storage.allocate(IdScope::Ticket)?.ticket()?;
    shell.storage.create_ticket(CreateTicket {
        id,
        initiative_id: view.id(),
        title: NonEmptyText::named("title", title)?,
        ticket_type,
        question: question.to_owned(),
        now: shell.now,
    })?;
    reread(shell, view.id(), id, output)
}

/// Take a ticket for this session.
pub fn claim(shell: &Shell<'_>, id: i64, output: &mut dyn Output) -> ShellResult<()> {
    let ticket_id = TicketId::new(id)?;
    let session_id = shell.session()?;
    let initiative_id = shell.writable_initiative()?;
    // The session is recorded first so that the view read next contains it, and
    // the core can therefore see what this session is already holding and has
    // already spent.
    shell.touch_session(initiative_id)?;

    let view = shell.view(initiative_id)?;
    let command = prepare_claim(
        ClaimInput {
            ticket_id,
            session_id,
            now: shell.now,
        },
        &view,
    )
    .map_err(claim_refusal)?;

    match shell.storage.claim_ticket(command)? {
        // Taking a ticket this session already holds changes nothing and is not
        // a mistake, so both answers print the ticket.
        ClaimOutcome::Claimed | ClaimOutcome::AlreadyHeld => {
            reread(shell, initiative_id, ticket_id, output)
        }
        ClaimOutcome::Conflict(conflict) => Err(claim_refusal(conflict)),
    }
}

/// Settle a ticket this session holds.
pub fn resolve(
    shell: &Shell<'_>,
    id: i64,
    source: &TextSource,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let ticket_id = TicketId::new(id)?;
    let resolution = input::read_resolution(shell.environment, source)?;
    let session_id = shell.session()?;
    let initiative_id = shell.writable_initiative()?;
    shell.touch_session(initiative_id)?;

    let view = shell.view(initiative_id)?;
    let decision_id = shell.storage.allocate(IdScope::Decision)?.decision()?;
    let command = prepare_resolution(
        ResolveInput {
            ticket_id,
            session_id,
            decision_id,
            resolution,
            now: shell.now,
        },
        &view,
    )
    .map_err(resolve_refusal)?;

    match shell.storage.resolve_ticket(command)? {
        ResolveOutcome::Resolved => reread(shell, initiative_id, ticket_id, output),
        ResolveOutcome::Conflict(conflict) => Err(resolve_refusal(conflict)),
    }
}

/// Repair the recorded text of a settled ticket.
///
/// No session is involved. Amending corrects a transcription fault, so it needs
/// no claim and grants none.
pub fn amend(
    shell: &Shell<'_>,
    id: i64,
    source: &TextSource,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let ticket_id = TicketId::new(id)?;
    let resolution = input::read_resolution(shell.environment, source)?;
    let view = shell.writable_view()?;
    let command = prepare_amend(
        AmendInput {
            ticket_id,
            resolution,
            now: shell.now,
        },
        &view,
    )
    .map_err(amend_refusal)?;

    match shell.storage.amend_ticket(command)? {
        AmendOutcome::Amended => reread(shell, view.id(), ticket_id, output),
        AmendOutcome::Conflict(conflict) => Err(amend_refusal(conflict)),
    }
}

/// Make one ticket wait on another.
pub fn block(shell: &Shell<'_>, id: i64, blocker: i64, output: &mut dyn Output) -> ShellResult<()> {
    let ticket_id = TicketId::new(id)?;
    let blocker_id = TicketId::new(blocker)?;
    let view = shell.writable_view()?;
    let command = prepare_dependency(
        DependencyInput {
            ticket_id,
            blocker_id,
            now: shell.now,
        },
        &view,
    )
    .map_err(dependency_refusal)?;

    match shell.storage.insert_dependency(command)? {
        InsertDependencyOutcome::Inserted | InsertDependencyOutcome::AlreadyPresent => {
            reread(shell, view.id(), ticket_id, output)
        }
        InsertDependencyOutcome::Conflict(conflict) => Err(dependency_refusal(conflict)),
    }
}

/// Read the initiative again and print one ticket from it.
///
/// Every writing command ends this way. What is printed is what the store now
/// holds — including anything another session committed in the meantime —
/// rather than what this command believes it wrote.
fn reread(
    shell: &Shell<'_>,
    initiative_id: InitiativeId,
    ticket_id: TicketId,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let view = shell.view(initiative_id)?;
    let document = shell.ticket_document(&view, ticket_id)?;
    output.text(&document)
}
