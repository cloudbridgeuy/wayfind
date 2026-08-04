//! `wayfind ticket ID`: one ticket in full.

use std::fmt::Write as _;

use super::front_matter::FrontMatter;
use super::rows::{AttachmentRow, ReferencedAttachmentRow};
use crate::format::{clamp_gist, flatten_lines, format_size};
use crate::id::TicketId;
use crate::model::{TicketStatusLabel, TicketType};
use crate::time::Timestamp;

/// Everything `wayfind ticket ID` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketView {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
    /// Where it stands.
    pub status: TicketStatusLabel,
    /// The question it asks.
    pub question: String,
    /// The decision, once there is one.
    pub resolution: Option<String>,
    /// When the decision text was last repaired.
    pub amended_at: Option<Timestamp>,
    /// Tickets it waits on, in identifier order.
    pub blocked_by: Vec<TicketId>,
    /// Attachments it owns.
    pub attachments: Vec<AttachmentRow>,
    /// Attachments it points at.
    pub referenced: Vec<ReferencedAttachmentRow>,
}

/// Render one ticket.
pub fn render_ticket(model: &TicketView) -> String {
    let mut out = FrontMatter::new("ticket")
        .number("id", model.id.get())
        .text("title", &model.title)
        .text("type", model.ticket_type.as_str())
        .text("status", model.status.as_str())
        .ids("blocked_by", model.blocked_by.clone())
        .number(
            "attachments",
            i64::try_from(model.attachments.len()).unwrap_or(i64::MAX),
        )
        .number(
            "referenced",
            i64::try_from(model.referenced.len()).unwrap_or(i64::MAX),
        )
        .optional_text(
            "amended_at",
            model.amended_at.as_ref().map(Timestamp::to_string),
        )
        .render();
    out.push('\n');

    let _ = write!(
        out,
        "# {}\n\n## Question\n\n{}\n",
        model.title, model.question
    );
    if let Some(resolution) = &model.resolution {
        let _ = write!(out, "\n## Resolution\n\n{resolution}\n");
    }

    if !model.attachments.is_empty() {
        out.push_str("\n## Attachments\n\nRun `wayfind attach show ID` for the full document.\n\n");
        for row in &model.attachments {
            let _ = writeln!(
                out,
                "- [{}] {} ({}) — {}",
                row.id,
                flatten_lines(&row.name),
                format_size(row.bytes),
                clamp_gist(&row.description)
            );
        }
    }

    if !model.referenced.is_empty() {
        out.push_str("\n## Referenced attachments\n\n");
        for row in &model.referenced {
            let _ = writeln!(
                out,
                "- [{}] {} ({}) — from ticket {} — {}",
                row.id,
                flatten_lines(&row.name),
                format_size(row.bytes),
                row.owner,
                clamp_gist(&row.description)
            );
        }
    }
    out
}
