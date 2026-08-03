//! Documents filed against a ticket.
//!
//! An attachment is a piece of text — a transcript, a benchmark table, a
//! specification — that belongs to one ticket and can be pointed at from
//! others. It lives in the same database as everything else, so a handoff is
//! one file and nothing goes missing between machines.
//!
//! Two of these commands address a document rather than a map, so they resolve
//! their ticket through the project rather than through whichever initiative
//! happens to be current: an attachment does not stop existing because the work
//! moved on.

use wayfind_core::{
    render_attachment_header, render_attachment_list, AddAttachmentReference, AttachmentId,
    AttachmentListView, AttachmentName, AttachmentView, Consistency, IdScope, InitiativeId,
    OwnedAttachmentRow, ReferenceOutcome, RemoveAttachmentOutcome, RemoveAttachmentReference,
    StoreAttachment, TicketId,
};

use super::{reference_refusal, Shell};
use crate::args::TextSource;
use crate::error::{ShellError, ShellResult};
use crate::input;
use crate::output::Output;

/// File a document against a ticket of the initiative in play.
pub fn add(
    shell: &Shell<'_>,
    ticket: i64,
    source: &TextSource,
    description: &str,
    chosen_name: Option<&str>,
    move_source: bool,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let ticket_id = TicketId::new(ticket)?;
    let name = AttachmentName::new(input::attachment_name(source, chosen_name)?)?;

    let session_id = shell.session()?;
    let initiative_id = shell.writable_initiative()?;
    shell.touch_session(initiative_id)?;

    let view = shell.view(initiative_id)?;
    if view.ticket(ticket_id).is_none() {
        return Err(ShellError::refused(format!(
            "ticket {ticket_id} is not in initiative {initiative_id}"
        )));
    }

    let content = input::read_content(shell.environment, source)?;
    let id = shell.storage.allocate(IdScope::Attachment)?.attachment()?;
    let stored = shell.storage.store_attachment(
        StoreAttachment {
            id,
            ticket_id,
            name,
            description: description.to_owned(),
            session_id: Some(session_id),
            now: shell.now,
        },
        content.bytes(),
    )?;

    if move_source {
        // The source is deleted only once the store has been asked what it
        // actually holds. A document half-written and then deleted from disk is
        // the one failure this command must never cause.
        if let TextSource::File(path) = source {
            if stored.byte_size != content.size() {
                return Err(ShellError::refused(
                    "stored size does not match the source; keeping the file",
                ));
            }
            shell.environment.remove_file(path)?;
        }
    }

    let view = shell.view(initiative_id)?;
    let document = shell.ticket_document(&view, ticket_id)?;
    output.text(&document)
}

/// Point a ticket at a document another ticket owns.
pub fn add_reference(
    shell: &Shell<'_>,
    ticket: i64,
    attachment: i64,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let ticket_id = TicketId::new(ticket)?;
    let attachment_id = AttachmentId::new(attachment)?;
    let view = shell.writable_view()?;
    let owner = reference_participants(shell, &view, ticket_id, attachment_id)?;
    if owner == ticket_id {
        return Err(ShellError::refused(format!(
            "ticket {ticket_id} owns attachment {attachment_id}"
        )));
    }

    match shell.storage.add_reference(AddAttachmentReference {
        attachment_id,
        ticket_id,
        now: shell.now,
    })? {
        ReferenceOutcome::Conflict(conflict) => Err(reference_refusal(conflict)),
        _ => {
            let view = shell.view(view.id())?;
            let document = shell.ticket_document(&view, ticket_id)?;
            output.text(&document)
        }
    }
}

/// Stop a ticket pointing at a document.
pub fn remove_reference(
    shell: &Shell<'_>,
    ticket: i64,
    attachment: i64,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let ticket_id = TicketId::new(ticket)?;
    let attachment_id = AttachmentId::new(attachment)?;
    let view = shell.writable_view()?;
    reference_participants(shell, &view, ticket_id, attachment_id)?;

    match shell.storage.remove_reference(RemoveAttachmentReference {
        attachment_id,
        ticket_id,
    })? {
        ReferenceOutcome::Conflict(conflict) => Err(reference_refusal(conflict)),
        _ => {
            let view = shell.view(view.id())?;
            let document = shell.ticket_document(&view, ticket_id)?;
            output.text(&document)
        }
    }
}

/// List the documents filed in the initiative, or against one ticket of it.
pub fn list(shell: &Shell<'_>, ticket: Option<i64>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.readable_view()?;
    let only = match ticket {
        Some(raw) => {
            let ticket_id = TicketId::new(raw)?;
            if view.ticket(ticket_id).is_none() {
                return Err(ShellError::refused(format!(
                    "ticket {ticket_id} is not in initiative {}",
                    view.id()
                )));
            }
            Some(ticket_id)
        }
        None => None,
    };

    let attachments = shell
        .storage
        .attachment_index(view.id(), Consistency::Strong)?
        .into_iter()
        .filter(|document| only.is_none_or(|wanted| document.ticket_id == wanted))
        .map(|document| OwnedAttachmentRow {
            id: document.id,
            ticket_id: document.ticket_id,
            name: document.name,
            bytes: document.byte_size,
            description: document.description,
        })
        .collect();

    output.text(&render_attachment_list(&AttachmentListView {
        initiative_id: view.id(),
        attachments,
    }))
}

/// Print one document, with or without its front matter.
///
/// The content is written as bytes rather than as text, so a document that is
/// not valid UTF-8 — or that ends in a bare carriage return — comes out of
/// `--raw` exactly as it went in.
pub fn show(
    shell: &Shell<'_>,
    attachment: i64,
    raw: bool,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let attachment_id = AttachmentId::new(attachment)?;
    let metadata = document_in_project(shell, attachment_id)?;
    let content = shell
        .storage
        .read_attachment(attachment_id)?
        .ok_or_else(|| ShellError::refused(format!("no attachment {attachment_id}")))?;

    if !raw {
        output.text(&render_attachment_header(&AttachmentView {
            id: metadata.id,
            name: metadata.name,
            ticket_id: metadata.ticket_id,
            bytes: metadata.byte_size,
            created_at: metadata.created_at,
            description: metadata.description,
        }))?;
    }
    output.bytes(&content)?;
    // The terminating line feed the store does not keep. Putting it back is
    // what makes `attach show --raw > file` reproduce the file that was filed.
    output.text("\n")
}

/// Delete a document and every reference to it.
pub fn remove(shell: &Shell<'_>, attachment: i64, output: &mut dyn Output) -> ShellResult<()> {
    let attachment_id = AttachmentId::new(attachment)?;
    let metadata = document_in_project(shell, attachment_id)?;
    let ticket = shell.ticket_in_project(metadata.ticket_id)?;

    match shell.storage.remove_attachment(attachment_id)? {
        RemoveAttachmentOutcome::Removed { .. } | RemoveAttachmentOutcome::NotFound => {
            let view = shell.view(ticket.initiative_id)?;
            let document = shell.ticket_document(&view, ticket.id)?;
            output.text(&document)
        }
    }
}

/// The document, once it is known to belong to this project.
fn document_in_project(
    shell: &Shell<'_>,
    attachment_id: AttachmentId,
) -> ShellResult<wayfind_core::AttachmentMetadata> {
    let metadata = shell
        .storage
        .attachment_metadata(attachment_id, Consistency::Strong)?
        .ok_or_else(|| ShellError::refused(format!("no attachment {attachment_id}")))?;
    shell.ticket_in_project(metadata.ticket_id)?;
    Ok(metadata)
}

/// Check that both ends of a reference are inside the initiative in play, and
/// answer which ticket owns the document.
fn reference_participants(
    shell: &Shell<'_>,
    view: &wayfind_core::InitiativeView,
    ticket_id: TicketId,
    attachment_id: AttachmentId,
) -> ShellResult<TicketId> {
    let outside = |id: TicketId, initiative: InitiativeId| {
        ShellError::refused(format!("ticket {id} is not in initiative {initiative}"))
    };
    if view.ticket(ticket_id).is_none() {
        return Err(outside(ticket_id, view.id()));
    }
    let metadata = shell
        .storage
        .attachment_metadata(attachment_id, Consistency::Strong)?
        .ok_or_else(|| ShellError::refused(format!("no attachment {attachment_id}")))?;
    if view.ticket(metadata.ticket_id).is_none() {
        return Err(outside(metadata.ticket_id, view.id()));
    }
    Ok(metadata.ticket_id)
}
