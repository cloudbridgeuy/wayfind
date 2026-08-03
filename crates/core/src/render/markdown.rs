//! Markdown writers shared by more than one document.

use std::fmt::Write as _;

use super::rows::OwnedAttachmentRow;
use crate::format::{clamp_gist, flatten_lines, format_size};

pub(super) fn push_notes(out: &mut String, notes: &[String]) {
    for note in notes {
        let _ = writeln!(out, "- {}", flatten_lines(note));
    }
}

pub(super) fn push_attachment_table(out: &mut String, rows: &[OwnedAttachmentRow]) {
    out.push_str("| ID | Ticket | Name | Size | Description |\n| --- | --- | --- | --- | --- |\n");
    for row in rows {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            row.id,
            row.ticket_id,
            cell(&row.name),
            format_size(row.bytes),
            cell(&clamp_gist(&row.description))
        );
    }
}

/// Prepare a value for a Markdown table cell.
///
/// A pipe inside a cell would end it early and shift every later column, so the
/// pipe is escaped. The shell script left it raw and produced a table that read
/// wrong whenever a title carried one.
pub(super) fn cell(text: &str) -> String {
    flatten_lines(text).replace('|', "\\|")
}

pub(super) fn count(len: usize) -> i64 {
    i64::try_from(len).unwrap_or(i64::MAX)
}
