//! Making, reading, and closing a map.
//!
//! An initiative is the unit of wayfinding work: one destination, one graph of
//! tickets, one handoff. The commands here create it, describe it in the three
//! ways an agent asks for it — the map, the tree, the handoff — record what is
//! still unknown or deliberately out of it, and close it when nothing is left.

use std::path::Path;

use wayfind_core::{
    prepare_clear, render_handoff, render_init, render_initiative_cleared, render_tree, AddFogNote,
    AddScopeExclusion, ClearOutcome, Consistency, CreateInitiative, FullDecision, HandoffView,
    IdScope, InitiativeView, NonEmptyText, OwnedAttachmentRow, ProjectKey, TreeView, UnresolvedRow,
};

use super::{clear_refusal, header, Shell};
use crate::error::ShellResult;
use crate::output::Output;

/// Create the database and record this project in it.
///
/// This is the one command that may bring a file into existence, so it is also
/// the one that reports where that file is.
pub fn init(
    shell: &Shell<'_>,
    database: &Path,
    project: &ProjectKey,
    output: &mut dyn Output,
) -> ShellResult<()> {
    shell.ensure_project()?;
    output.text(&render_init(&database.display().to_string(), project))
}

/// Start a new initiative and show its map.
pub fn create(
    shell: &Shell<'_>,
    name: &str,
    destination: &str,
    notes: Option<&str>,
    output: &mut dyn Output,
) -> ShellResult<()> {
    shell.ensure_project()?;
    let id = shell.storage.allocate(IdScope::Initiative)?.initiative()?;
    shell.storage.create_initiative(CreateInitiative {
        id,
        project_key: shell.project.clone(),
        name: NonEmptyText::named("name", name)?,
        destination: NonEmptyText::named("destination", destination)?,
        notes: notes.unwrap_or_default().to_owned(),
        now: shell.now,
    })?;
    let view = shell.view(id)?;
    let document = shell.map_document(&view)?;
    output.text(&document)
}

/// Close the initiative in play.
pub fn clear(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.writable_view()?;
    let command = prepare_clear(&view, shell.now).map_err(|conflict| clear_refusal(&conflict))?;
    match shell.storage.clear_initiative(command)? {
        // Closing something already closed changed nothing, and saying so twice
        // is what an idempotent command should do.
        ClearOutcome::Cleared | ClearOutcome::AlreadyClear => {
            output.text(&render_initiative_cleared(view.id()))
        }
        ClearOutcome::Conflict(conflict) => Err(clear_refusal(&conflict)),
    }
}

/// Show the map: where the work is going and what can be picked up now.
pub fn map(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.readable_view()?;
    let document = shell.map_document(&view)?;
    output.text(&document)
}

/// Show the dependency tree.
pub fn tree(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.readable_view()?;
    let model = TreeView::new(
        view.initiative().name.clone(),
        view.tickets(),
        view.dependencies(),
    );
    output.text(&render_tree(&model))
}

/// Show the handoff: every decision in full, and everything still open.
pub fn handoff(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.readable_view()?;
    let model = handoff_view(shell, &view)?;
    output.text(&render_handoff(&model))
}

/// Record something the initiative has not settled yet.
pub fn fog(shell: &Shell<'_>, note: &str, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.writable_view()?;
    let id = shell.storage.allocate(IdScope::FogNote)?.note()?;
    shell.storage.add_fog_note(AddFogNote {
        id,
        initiative_id: view.id(),
        note: NonEmptyText::named("note", note)?,
        now: shell.now,
    })?;
    // The map is re-read rather than patched, so what is printed is what the
    // store now holds and not what this command believes it wrote.
    let view = shell.view(view.id())?;
    let document = shell.map_document(&view)?;
    output.text(&document)
}

/// Record something the initiative is deliberately not doing.
pub fn exclude(shell: &Shell<'_>, note: &str, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.writable_view()?;
    let id = shell.storage.allocate(IdScope::ScopeExclusion)?.note()?;
    shell.storage.add_scope_exclusion(AddScopeExclusion {
        id,
        initiative_id: view.id(),
        note: NonEmptyText::named("note", note)?,
        now: shell.now,
    })?;
    let view = shell.view(view.id())?;
    let document = shell.map_document(&view)?;
    output.text(&document)
}

/// Gather everything a handoff document reports.
fn handoff_view(shell: &Shell<'_>, view: &InitiativeView) -> ShellResult<HandoffView> {
    let id = view.id();

    let unresolved = view
        .tickets()
        .iter()
        .filter(|ticket| ticket.state.is_unresolved())
        .map(|ticket| UnresolvedRow {
            id: ticket.id,
            title: ticket.title.clone(),
            ticket_type: ticket.ticket_type,
            status: ticket.state.label(),
        })
        .collect();

    // Decision order, not ticket order: a handoff is read as a narrative of how
    // the work was settled.
    let mut decisions = Vec::new();
    for decision in shell.storage.decisions(id, Consistency::Strong)? {
        let Some(ticket) = view.ticket(decision.ticket_id) else {
            continue;
        };
        decisions.push(FullDecision {
            ticket_id: ticket.id,
            title: ticket.title.clone(),
            question: ticket.question.clone(),
            resolution: ticket.state.resolution().unwrap_or_default().to_owned(),
        });
    }

    let attachments = shell
        .storage
        .attachment_index(id, Consistency::Strong)?
        .into_iter()
        .map(|document| OwnedAttachmentRow {
            id: document.id,
            ticket_id: document.ticket_id,
            name: document.name,
            bytes: document.byte_size,
            description: document.description,
        })
        .collect();

    Ok(HandoffView {
        initiative: header(view.initiative()),
        unresolved,
        decisions,
        fog: shell
            .storage
            .fog_notes(id, Consistency::Strong)?
            .into_iter()
            .map(|note| note.note)
            .collect(),
        exclusions: shell
            .storage
            .scope_exclusions(id, Consistency::Strong)?
            .into_iter()
            .map(|note| note.note)
            .collect(),
        attachments,
    })
}
