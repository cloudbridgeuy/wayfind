//! `wayfind attach list` and the header `attach show` prints.

use std::fmt::Write as _;

use super::front_matter::FrontMatter;
use super::markdown::{count, push_attachment_table};
use super::rows::OwnedAttachmentRow;
use crate::id::{AttachmentId, InitiativeId, TicketId};
use crate::time::Timestamp;

/// Everything `wayfind attach list` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentListView {
    /// The initiative the attachments belong to.
    pub initiative_id: InitiativeId,
    /// The attachments, in identifier order.
    pub attachments: Vec<OwnedAttachmentRow>,
}

/// Render the attachment table.
pub fn render_attachment_list(model: &AttachmentListView) -> String {
    let mut out = FrontMatter::new("attachments")
        .number("initiative_id", model.initiative_id.get())
        .number("count", count(model.attachments.len()))
        .render();
    out.push_str("\n# Attachments\n\n");
    push_attachment_table(&mut out, &model.attachments);
    out
}

/// The heading `wayfind attach show` prints above the stored document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentView {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// Its file name.
    pub name: String,
    /// The ticket that owns it.
    pub ticket_id: TicketId,
    /// Its size in bytes.
    pub bytes: u64,
    /// When it was stored.
    pub created_at: Timestamp,
    /// What it is for.
    pub description: String,
}

/// Render the heading of a stored document, up to the rule that precedes it.
///
/// The content itself is not rendered here. It is written to the output as the
/// bytes that were stored, because an attachment is a document the operator put
/// in, and a renderer that re-encoded it would hand back something else.
pub fn render_attachment_header(model: &AttachmentView) -> String {
    let mut out = FrontMatter::new("attachment")
        .number("id", model.id.get())
        .text("name", &model.name)
        .number("ticket_id", model.ticket_id.get())
        .number("bytes", i64::try_from(model.bytes).unwrap_or(i64::MAX))
        .text("created_at", model.created_at.to_string())
        .render();
    let _ = write!(
        out,
        "\n# {}\n\n{}\n\n---\n\n",
        model.name, model.description
    );
    out
}
