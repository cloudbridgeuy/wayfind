//! `wayfind handoff`: the digest the next agent reads.

use std::fmt::Write as _;

use super::front_matter::FrontMatter;
use super::markdown::{count, push_attachment_table, push_notes};
use super::rows::{FullDecision, InitiativeHeader, OwnedAttachmentRow, UnresolvedRow};
use crate::format::flatten_lines;

/// Everything `wayfind handoff` prints.
///
/// This is the digest the next skill reads, so the decisions appear in full
/// rather than as gists: whoever picks the work up should not have to go back
/// to the database to find out what was settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffView {
    /// The initiative being handed off.
    pub initiative: InitiativeHeader,
    /// Tickets still open or claimed, in identifier order.
    pub unresolved: Vec<UnresolvedRow>,
    /// Every decision, in decision order.
    pub decisions: Vec<FullDecision>,
    /// Questions the map has not answered yet.
    pub fog: Vec<String>,
    /// Questions the map has deliberately dropped.
    pub exclusions: Vec<String>,
    /// Every attachment in the initiative, in identifier order.
    pub attachments: Vec<OwnedAttachmentRow>,
}

/// Render `wayfind handoff`.
pub fn render_handoff(model: &HandoffView) -> String {
    let header = &model.initiative;
    let mut out = FrontMatter::new("handoff")
        .number("initiative_id", header.id.get())
        .text("name", &header.name)
        .text("status", header.status.as_str())
        .number("decisions", count(model.decisions.len()))
        .number("unresolved", count(model.unresolved.len()))
        .number("attachments", count(model.attachments.len()))
        .render();
    out.push('\n');

    let _ = write!(
        out,
        "# Handoff: {}\n\n## Destination\n\n{}\n\n",
        header.name, header.destination
    );
    if !header.notes.is_empty() {
        let _ = write!(out, "## Notes\n\n{}\n\n", header.notes);
    }

    if !model.unresolved.is_empty() {
        out.push_str(
            "## Unresolved tickets\n\nThe map is not clear. These tickets still need decisions.\n\n",
        );
        for row in &model.unresolved {
            let _ = writeln!(
                out,
                "- [{}] {} ({}, {})",
                row.id,
                flatten_lines(&row.title),
                row.ticket_type,
                row.status
            );
        }
        out.push('\n');
    }

    out.push_str("## Decisions\n\nFull text, in decision order. Treat each one as settled.\n");
    for decision in &model.decisions {
        let _ = write!(
            out,
            "\n### [{}] {}\n\n**Question.** {}\n\n{}\n",
            decision.ticket_id, decision.title, decision.question, decision.resolution
        );
    }

    out.push_str("\n## Not yet specified\n\n");
    push_notes(&mut out, &model.fog);
    out.push_str("\n## Out of scope\n\n");
    push_notes(&mut out, &model.exclusions);

    if !model.attachments.is_empty() {
        out.push_str("\n## Attachments\n\nRun `wayfind attach show ID` for the full document.\n\n");
        push_attachment_table(&mut out, &model.attachments);
    }
    out
}
