//! Turning a read view into the document an operator or an agent reads.
//!
//! Every command that prints a document builds a view model here and hands it
//! to a render function. The models hold only what the document shows, so a
//! rendering test states a situation instead of standing up a database, and a
//! change of wording cannot quietly change what was read.
//!
//! Most documents are TOML front matter followed by Markdown. The front matter
//! is what a program reads: a `kind` key first, then the few facts worth acting
//! on. The Markdown below it is what a person reads. Both come out of one pass,
//! so the two halves cannot disagree.
//!
//! Four outputs are deliberately not front matter, because their readers are
//! not front-matter readers: `init` and `initiative clear` report one line to a
//! human, `tree` draws a diagram, and `dump --csv` writes records for a
//! spreadsheet.

use std::fmt::Write as _;

use crate::error::{Error, Result};
use crate::format::{clamp_gist, flatten_lines, format_size, toml_string};
use crate::id::{AttachmentId, InitiativeId, ProjectKey, SessionId, TicketId};
use crate::model::{
    BlockedReason, InitiativeState, PersistedInitiativeStatus, TicketStatusLabel, TicketType,
};
use crate::search::SearchHit;
use crate::time::Timestamp;

// ---------------------------------------------------------------------------
// Front matter
// ---------------------------------------------------------------------------

/// One front-matter value.
///
/// Only three shapes ever appear, and each renders one way, so no caller has to
/// decide how a number or a title should be quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// A count or an identifier.
    Number(i64),
    /// Text, quoted and escaped as a TOML basic string.
    Text(String),
    /// A list of ticket identifiers, such as `blocked_by`.
    Ids(Vec<TicketId>),
}

/// The `+++` block at the top of a document.
///
/// Keys keep insertion order. The order is part of the output, so it is decided
/// once, where the document is built, rather than by a map's iteration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    entries: Vec<(&'static str, Field)>,
}

impl FrontMatter {
    /// Start a block. `kind` is always the first key, so a reader can dispatch
    /// on it without parsing the rest.
    pub fn new(kind: &'static str) -> Self {
        Self {
            entries: vec![("kind", Field::Text(kind.to_string()))],
        }
    }

    /// Add a numeric key.
    pub fn number(mut self, key: &'static str, value: impl Into<i64>) -> Self {
        self.entries.push((key, Field::Number(value.into())));
        self
    }

    /// Add a text key.
    pub fn text(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.entries.push((key, Field::Text(value.into())));
        self
    }

    /// Add a list of ticket identifiers.
    pub fn ids(mut self, key: &'static str, value: Vec<TicketId>) -> Self {
        self.entries.push((key, Field::Ids(value)));
        self
    }

    /// Add a text key only when there is something to say.
    pub fn optional_text(self, key: &'static str, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.text(key, value),
            None => self,
        }
    }

    /// Render the block, `+++` fences included, ending in one newline.
    pub fn render(&self) -> String {
        let mut out = String::from("+++\n");
        for (key, value) in &self.entries {
            match value {
                Field::Number(number) => {
                    let _ = writeln!(out, "{key} = {number}");
                }
                Field::Text(text) => {
                    let _ = writeln!(out, "{key} = {}", toml_string(text));
                }
                Field::Ids(ids) => {
                    let joined = ids
                        .iter()
                        .map(|id| id.get().to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    let _ = writeln!(out, "{key} = [{joined}]");
                }
            }
        }
        out.push_str("+++\n");
        out
    }
}

// ---------------------------------------------------------------------------
// Guidance
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Shared view pieces
// ---------------------------------------------------------------------------

/// The initiative facts every map-shaped document repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiativeHeader {
    /// The initiative's identifier.
    pub id: InitiativeId,
    /// Its name.
    pub name: String,
    /// Where the work is going.
    pub destination: String,
    /// Free-form notes, empty when there are none.
    pub notes: String,
    /// Whether the operator has closed it.
    pub status: PersistedInitiativeStatus,
}

/// One ticket that can be picked up right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
}

/// One settled ticket, as the map lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRow {
    /// The ticket the decision belongs to.
    pub ticket_id: TicketId,
    /// The ticket's title.
    pub title: String,
    /// The decision text, clamped when rendered.
    pub gist: String,
}

/// One attachment owned by the ticket being shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRow {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// Its file name.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// What it is for.
    pub description: String,
}

/// One attachment another ticket owns and this one points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedAttachmentRow {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// Its file name.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// The ticket that owns it.
    pub owner: TicketId,
    /// What it is for.
    pub description: String,
}

// ---------------------------------------------------------------------------
// The map
// ---------------------------------------------------------------------------

/// Everything `wayfind map` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapView {
    /// The initiative being mapped.
    pub initiative: InitiativeHeader,
    /// Tickets available right now, in identifier order.
    pub frontier: Vec<FrontierRow>,
    /// Where the initiative stands, used for guidance when the frontier is
    /// empty.
    pub state: InitiativeState,
    /// Settled tickets, in decision order.
    pub decisions: Vec<DecisionRow>,
    /// Questions the map has not answered yet.
    pub fog: Vec<String>,
    /// Questions the map has deliberately dropped.
    pub exclusions: Vec<String>,
}

/// Render `wayfind map`.
pub fn render_map(model: &MapView) -> String {
    let header = &model.initiative;
    let mut out = FrontMatter::new("map")
        .number("initiative_id", header.id.get())
        .text("name", &header.name)
        .text("status", header.status.as_str())
        .render();
    out.push('\n');

    let _ = write!(
        out,
        "# {}\n\n## Destination\n\n{}\n\n",
        header.name, header.destination
    );
    if !header.notes.is_empty() {
        let _ = write!(out, "## Notes\n\n{}\n\n", header.notes);
    }

    out.push_str("## Frontier\n\n");
    if model.frontier.is_empty() {
        let _ = writeln!(out, "{}", state_guidance(&model.state, header.id));
    } else {
        for row in &model.frontier {
            let _ = writeln!(
                out,
                "- [{}] {} ({})",
                row.id,
                flatten_lines(&row.title),
                row.ticket_type
            );
        }
    }

    out.push_str(
        "\n## Decisions so far\n\nGists are clamped. Run `wayfind ticket ID` for the full decision.\n\n",
    );
    for row in &model.decisions {
        let _ = writeln!(
            out,
            "- [{}] {} — {}",
            row.ticket_id,
            flatten_lines(&row.title),
            clamp_gist(&row.gist)
        );
    }

    out.push_str("\n## Not yet specified\n\n");
    push_notes(&mut out, &model.fog);
    out.push_str("\n## Out of scope\n\n");
    push_notes(&mut out, &model.exclusions);
    out
}

fn push_notes(out: &mut String, notes: &[String]) {
    for note in notes {
        let _ = writeln!(out, "- {}", flatten_lines(note));
    }
}

// ---------------------------------------------------------------------------
// One ticket
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// The handoff
// ---------------------------------------------------------------------------

/// One ticket still waiting for a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
    /// Whether anyone holds it.
    pub status: TicketStatusLabel,
}

/// One decision, in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullDecision {
    /// The ticket the decision belongs to.
    pub ticket_id: TicketId,
    /// The ticket's title.
    pub title: String,
    /// The question that was asked.
    pub question: String,
    /// The decision text, never clamped.
    pub resolution: String,
}

/// One attachment, as the handoff table lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedAttachmentRow {
    /// The attachment's identifier.
    pub id: AttachmentId,
    /// The ticket that owns it.
    pub ticket_id: TicketId,
    /// Its file name.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// What it is for.
    pub description: String,
}

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

fn push_attachment_table(out: &mut String, rows: &[OwnedAttachmentRow]) {
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
fn cell(text: &str) -> String {
    flatten_lines(text).replace('|', "\\|")
}

fn count(len: usize) -> i64 {
    i64::try_from(len).unwrap_or(i64::MAX)
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// One row of the active session table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// The session's identifier.
    pub id: SessionId,
    /// The ticket it holds, with that ticket's title.
    pub holding: Option<(TicketId, String)>,
    /// When it was last heard from.
    pub last_seen_at: Timestamp,
}

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

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// Everything `wayfind search` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchView {
    /// What was asked for.
    pub query: String,
    /// How many hits were asked for.
    pub limit: u32,
    /// How many hits were skipped.
    pub offset: u32,
    /// The hits, most relevant first.
    pub hits: Vec<SearchHit>,
}

/// Render search results.
pub fn render_search(model: &SearchView) -> String {
    let mut out = FrontMatter::new("search")
        .text("query", &model.query)
        .number("limit", i64::from(model.limit))
        .number("offset", i64::from(model.offset))
        .render();
    out.push_str("\n# Search results\n\n");
    for hit in &model.hits {
        let _ = writeln!(
            out,
            "- [{}] {} ({}) — {}",
            hit.ticket_id,
            flatten_lines(&hit.title),
            hit.status,
            flatten_lines(&hit.snippet)
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Records and one-line reports
// ---------------------------------------------------------------------------

/// One record of `wayfind dump --csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
    /// Where it stands.
    pub status: TicketStatusLabel,
    /// The question it asks, flattened to one line.
    pub question: String,
    /// The decision, flattened to one line, empty when there is none.
    pub resolution: String,
}

/// The header row of `wayfind dump --csv`.
pub const DUMP_HEADER: [&str; 6] = ["id", "title", "type", "status", "question", "resolution"];

/// Render `wayfind dump --csv`.
///
/// Question and decision text is flattened to one line, so one ticket is one
/// record whatever the author wrote. Quoting is the CSV writer's job, so a
/// title holding a comma or a quotation mark survives the round trip.
pub fn render_csv(rows: &[DumpRow]) -> Result<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(DUMP_HEADER)
        .map_err(|error| csv_failed(&error))?;
    for row in rows {
        writer
            .write_record([
                row.id.get().to_string().as_str(),
                flatten_lines(&row.title).as_str(),
                row.ticket_type.as_str(),
                row.status.as_str(),
                flatten_lines(&row.question).as_str(),
                flatten_lines(&row.resolution).as_str(),
            ])
            .map_err(|error| csv_failed(&error))?;
    }
    let bytes = writer.into_inner().map_err(|error| csv_failed(&error))?;
    String::from_utf8(bytes).map_err(|error| csv_failed(&error))
}

fn csv_failed(error: &dyn std::fmt::Display) -> Error {
    Error::invalid_value("csv record", error.to_string())
}

/// Render what `wayfind init` reports.
pub fn render_init(database: &str, project_key: &ProjectKey) -> String {
    format!("initialized {database} for {project_key}\n")
}

/// Render what `wayfind initiative clear` reports.
pub fn render_initiative_cleared(initiative: InitiativeId) -> String {
    format!("initiative {initiative} is clear\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{
        render_attachment_header, render_attachment_list, render_csv, render_handoff, render_init,
        render_initiative_cleared, render_map, render_next_unavailable, render_search,
        render_session_list, render_session_resume, render_ticket, state_guidance,
        AttachmentListView, AttachmentRow, AttachmentView, DecisionRow, DumpRow, FrontMatter,
        FrontierRow, FullDecision, HandoffView, InitiativeHeader, MapView, NextView,
        OwnedAttachmentRow, ReferencedAttachmentRow, SearchView, SessionListView,
        SessionResumeView, SessionRow, TicketView, UnresolvedRow,
    };
    use crate::id::{AttachmentId, InitiativeId, ProjectKey, SessionId, TicketId};
    use crate::model::{
        BlockedReason, FrontierTicket, InitiativeState, NonEmptyVec, PersistedInitiativeStatus,
        TicketStatusLabel, TicketType,
    };
    use crate::search::SearchHit;
    use crate::time::Timestamp;

    fn initiative_id() -> InitiativeId {
        InitiativeId::new(3).unwrap()
    }

    fn ticket_id(value: i64) -> TicketId {
        TicketId::new(value).unwrap()
    }

    fn attachment_id(value: i64) -> AttachmentId {
        AttachmentId::new(value).unwrap()
    }

    fn moment() -> Timestamp {
        Timestamp::from_str("2026-08-02 13:45:09").unwrap()
    }

    fn header() -> InitiativeHeader {
        InitiativeHeader {
            id: initiative_id(),
            name: "Cache the map".to_string(),
            destination: "A map that loads instantly".to_string(),
            notes: String::new(),
            status: PersistedInitiativeStatus::Working,
        }
    }

    fn ready() -> InitiativeState {
        InitiativeState::Ready {
            frontier: NonEmptyVec::try_from(vec![FrontierTicket {
                id: ticket_id(1),
                title: "Measure the load".to_string(),
                ticket_type: TicketType::Research,
            }])
            .unwrap(),
        }
    }

    // -- front matter ------------------------------------------------------

    #[test]
    fn front_matter_keeps_the_order_it_was_built_in() {
        let block = FrontMatter::new("ticket")
            .number("id", 4_i64)
            .text("title", "A \"quoted\" title")
            .ids("blocked_by", vec![ticket_id(2), ticket_id(9)])
            .render();
        assert_eq!(
            block,
            "+++\nkind = \"ticket\"\nid = 4\ntitle = \"A \\\"quoted\\\" title\"\nblocked_by = [2,9]\n+++\n"
        );
    }

    #[test]
    fn an_empty_identifier_list_renders_as_an_empty_list() {
        let block = FrontMatter::new("ticket")
            .ids("blocked_by", Vec::new())
            .render();
        assert!(block.contains("blocked_by = []\n"));
    }

    #[test]
    fn an_absent_optional_key_is_left_out_entirely() {
        let block = FrontMatter::new("ticket")
            .optional_text("amended_at", None::<String>)
            .render();
        assert!(!block.contains("amended_at"));
    }

    #[test]
    fn every_rendered_front_matter_block_parses_as_toml() {
        let awkward = TicketView {
            id: ticket_id(1),
            title: "tab\there and \"quotes\"".to_string(),
            ticket_type: TicketType::Task,
            status: TicketStatusLabel::Open,
            question: "why?".to_string(),
            resolution: None,
            amended_at: None,
            blocked_by: Vec::new(),
            attachments: Vec::new(),
            referenced: Vec::new(),
        };
        let rendered = render_ticket(&awkward);
        let block = rendered
            .split("+++\n")
            .nth(1)
            .expect("a front-matter block");
        let parsed: toml::Table = toml::from_str(block).expect("valid TOML");
        assert_eq!(parsed["title"].as_str(), Some("tab\there and \"quotes\""));
    }

    // -- guidance ----------------------------------------------------------

    #[test]
    fn guidance_answers_every_state() {
        assert!(state_guidance(&InitiativeState::Clear, initiative_id()).contains("is clear"));
        assert!(state_guidance(&InitiativeState::Complete, initiative_id())
            .contains("wayfind initiative clear"));
        assert!(
            state_guidance(&InitiativeState::Charting, initiative_id()).contains("no tickets yet")
        );
        assert!(state_guidance(&ready(), initiative_id()).contains("wayfind next"));
        assert!(state_guidance(
            &InitiativeState::Blocked(BlockedReason::ClaimsHoldFrontier { claimed: 2 }),
            initiative_id()
        )
        .contains("2 claimed ticket(s)"));
        assert!(state_guidance(
            &InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked),
            initiative_id()
        )
        .contains("wayfind tree"));
    }

    // -- the map -----------------------------------------------------------

    fn map_with(frontier: Vec<FrontierRow>, state: InitiativeState) -> MapView {
        MapView {
            initiative: header(),
            frontier,
            state,
            decisions: Vec::new(),
            fog: Vec::new(),
            exclusions: Vec::new(),
        }
    }

    #[test]
    fn a_map_lists_its_sections_in_order() {
        let mut model = map_with(
            vec![FrontierRow {
                id: ticket_id(1),
                title: "Measure the load".to_string(),
                ticket_type: TicketType::Research,
            }],
            ready(),
        );
        model.decisions.push(DecisionRow {
            ticket_id: ticket_id(2),
            title: "Pick a store".to_string(),
            gist: "SQLite,\nbundled".to_string(),
        });
        model.fog.push("How large does the cache get?".to_string());
        model.exclusions.push("Multi-user access".to_string());
        let rendered = render_map(&model);

        assert_eq!(
            rendered,
            concat!(
                "+++\n",
                "kind = \"map\"\n",
                "initiative_id = 3\n",
                "name = \"Cache the map\"\n",
                "status = \"working\"\n",
                "+++\n",
                "\n",
                "# Cache the map\n\n",
                "## Destination\n\nA map that loads instantly\n\n",
                "## Frontier\n\n",
                "- [1] Measure the load (research)\n",
                "\n## Decisions so far\n\n",
                "Gists are clamped. Run `wayfind ticket ID` for the full decision.\n\n",
                "- [2] Pick a store — SQLite, bundled\n",
                "\n## Not yet specified\n\n",
                "- How large does the cache get?\n",
                "\n## Out of scope\n\n",
                "- Multi-user access\n",
            )
        );
    }

    #[test]
    fn a_map_shows_its_notes_only_when_there_are_notes() {
        let mut model = map_with(Vec::new(), InitiativeState::Charting);
        assert!(!render_map(&model).contains("## Notes"));
        model.initiative.notes = "Read the old design first.".to_string();
        assert!(render_map(&model).contains("## Notes\n\nRead the old design first.\n\n"));
    }

    #[test]
    fn an_empty_frontier_prints_guidance_instead_of_a_list() {
        let model = map_with(Vec::new(), InitiativeState::Charting);
        let rendered = render_map(&model);
        assert!(rendered.contains("## Frontier\n\nInitiative 3 has no tickets yet."));
    }

    #[test]
    fn a_map_keeps_every_row_on_one_line() {
        let mut model = map_with(
            vec![FrontierRow {
                id: ticket_id(1),
                title: "Two\nlines".to_string(),
                ticket_type: TicketType::Task,
            }],
            ready(),
        );
        model.fog.push("Fog\nover\ntwo".to_string());
        let rendered = render_map(&model);
        assert!(rendered.contains("- [1] Two lines (task)\n"));
        assert!(rendered.contains("- Fog over two\n"));
    }

    #[test]
    fn a_long_decision_gist_is_clamped_but_the_handoff_keeps_it_whole() {
        let long = "x".repeat(400);
        let mut model = map_with(Vec::new(), InitiativeState::Complete);
        model.decisions.push(DecisionRow {
            ticket_id: ticket_id(2),
            title: "Long one".to_string(),
            gist: long.clone(),
        });
        assert!(render_map(&model).contains('…'));

        let handoff = HandoffView {
            initiative: header(),
            unresolved: Vec::new(),
            decisions: vec![FullDecision {
                ticket_id: ticket_id(2),
                title: "Long one".to_string(),
                question: "how long?".to_string(),
                resolution: long.clone(),
            }],
            fog: Vec::new(),
            exclusions: Vec::new(),
            attachments: Vec::new(),
        };
        assert!(render_handoff(&handoff).contains(&long));
    }

    // -- one ticket --------------------------------------------------------

    fn ticket_view() -> TicketView {
        TicketView {
            id: ticket_id(4),
            title: "Pick a store".to_string(),
            ticket_type: TicketType::Grilling,
            status: TicketStatusLabel::Open,
            question: "Which store?".to_string(),
            resolution: None,
            amended_at: None,
            blocked_by: Vec::new(),
            attachments: Vec::new(),
            referenced: Vec::new(),
        }
    }

    #[test]
    fn a_plain_ticket_renders_its_question_and_nothing_else() {
        assert_eq!(
            render_ticket(&ticket_view()),
            concat!(
                "+++\n",
                "kind = \"ticket\"\n",
                "id = 4\n",
                "title = \"Pick a store\"\n",
                "type = \"grilling\"\n",
                "status = \"open\"\n",
                "blocked_by = []\n",
                "attachments = 0\n",
                "referenced = 0\n",
                "+++\n",
                "\n",
                "# Pick a store\n\n## Question\n\nWhich store?\n",
            )
        );
    }

    #[test]
    fn a_settled_ticket_adds_its_decision_and_its_repair_time() {
        let mut model = ticket_view();
        model.status = TicketStatusLabel::Resolved;
        model.resolution = Some("SQLite".to_string());
        model.amended_at = Some(moment());
        let rendered = render_ticket(&model);
        assert!(rendered.contains("amended_at = \"2026-08-02 13:45:09\"\n"));
        assert!(rendered.ends_with("\n## Resolution\n\nSQLite\n"));
    }

    #[test]
    fn a_blocked_ticket_names_its_blockers_in_the_front_matter() {
        let mut model = ticket_view();
        model.blocked_by = vec![ticket_id(2), ticket_id(7)];
        assert!(render_ticket(&model).contains("blocked_by = [2,7]\n"));
    }

    #[test]
    fn attachment_sections_appear_only_when_there_are_attachments() {
        let mut model = ticket_view();
        assert!(!render_ticket(&model).contains("## Attachments"));

        model.attachments.push(AttachmentRow {
            id: attachment_id(1),
            name: "notes.md".to_string(),
            bytes: 2048,
            description: "The old design".to_string(),
        });
        model.referenced.push(ReferencedAttachmentRow {
            id: attachment_id(5),
            name: "bench.txt".to_string(),
            bytes: 512,
            owner: ticket_id(9),
            description: "Timings".to_string(),
        });
        let rendered = render_ticket(&model);
        assert!(rendered.contains("attachments = 1\nreferenced = 1\n"));
        assert!(rendered.contains("- [1] notes.md (2.0 KB) — The old design\n"));
        assert!(rendered.contains("- [5] bench.txt (512 B) — from ticket 9 — Timings\n"));
    }

    // -- the handoff -------------------------------------------------------

    #[test]
    fn a_handoff_counts_what_it_carries_and_warns_about_open_work() {
        let model = HandoffView {
            initiative: header(),
            unresolved: vec![UnresolvedRow {
                id: ticket_id(1),
                title: "Measure the load".to_string(),
                ticket_type: TicketType::Research,
                status: TicketStatusLabel::Claimed,
            }],
            decisions: vec![FullDecision {
                ticket_id: ticket_id(2),
                title: "Pick a store".to_string(),
                question: "Which store?".to_string(),
                resolution: "SQLite, bundled.".to_string(),
            }],
            fog: vec!["How large?".to_string()],
            exclusions: Vec::new(),
            attachments: vec![OwnedAttachmentRow {
                id: attachment_id(1),
                ticket_id: ticket_id(2),
                name: "bench.txt".to_string(),
                bytes: 1024,
                description: "Timings".to_string(),
            }],
        };
        let rendered = render_handoff(&model);
        assert!(rendered.contains("decisions = 1\nunresolved = 1\nattachments = 1\n"));
        assert!(rendered.contains("# Handoff: Cache the map\n"));
        assert!(rendered.contains("## Unresolved tickets\n"));
        assert!(rendered.contains("- [1] Measure the load (research, claimed)\n"));
        assert!(rendered.contains(
            "\n### [2] Pick a store\n\n**Question.** Which store?\n\nSQLite, bundled.\n"
        ));
        assert!(rendered.contains("| 1 | 2 | bench.txt | 1.0 KB | Timings |\n"));
    }

    #[test]
    fn a_clear_handoff_leaves_out_the_unresolved_section() {
        let model = HandoffView {
            initiative: header(),
            unresolved: Vec::new(),
            decisions: Vec::new(),
            fog: Vec::new(),
            exclusions: Vec::new(),
            attachments: Vec::new(),
        };
        let rendered = render_handoff(&model);
        assert!(!rendered.contains("## Unresolved tickets"));
        assert!(!rendered.contains("## Attachments"));
        assert!(rendered.ends_with("## Out of scope\n\n"));
    }

    // -- tables ------------------------------------------------------------

    #[test]
    fn a_table_cell_cannot_end_its_row_early() {
        let model = AttachmentListView {
            initiative_id: initiative_id(),
            attachments: vec![OwnedAttachmentRow {
                id: attachment_id(1),
                ticket_id: ticket_id(2),
                name: "a|b.txt".to_string(),
                bytes: 10,
                description: "one | two".to_string(),
            }],
        };
        let rendered = render_attachment_list(&model);
        assert!(rendered.contains("| 1 | 2 | a\\|b.txt | 10 B | one \\| two |\n"));
    }

    #[test]
    fn the_session_table_reports_who_is_holding_what() {
        let model = SessionListView {
            initiative_id: initiative_id(),
            sessions: vec![
                SessionRow {
                    id: SessionId::new("session-a").unwrap(),
                    holding: Some((ticket_id(4), "Pick a store".to_string())),
                    last_seen_at: moment(),
                },
                SessionRow {
                    id: SessionId::new("session-b").unwrap(),
                    holding: None,
                    last_seen_at: moment(),
                },
            ],
        };
        let rendered = render_session_list(&model);
        assert!(rendered.contains("count = 2\n"));
        assert!(
            rendered.contains("| session-a | working | [4] Pick a store | 2026-08-02 13:45:09 |\n")
        );
        assert!(rendered.contains("| session-b | ready | — | 2026-08-02 13:45:09 |\n"));
    }

    #[test]
    fn an_empty_session_table_still_prints_its_heading_row() {
        let model = SessionListView {
            initiative_id: initiative_id(),
            sessions: Vec::new(),
        };
        let rendered = render_session_list(&model);
        assert!(rendered.contains("count = 0\n"));
        assert!(rendered.ends_with("| --- | --- | --- | --- |\n"));
    }

    // -- sessions and next -------------------------------------------------

    #[test]
    fn a_resumed_session_reports_where_the_initiative_stands() {
        let model = SessionResumeView {
            session_id: SessionId::new("session-a").unwrap(),
            initiative_id: initiative_id(),
            state: InitiativeState::Charting,
        };
        assert_eq!(
            render_session_resume(&model),
            concat!(
                "+++\n",
                "kind = \"session\"\n",
                "id = \"session-a\"\n",
                "initiative_id = 3\n",
                "status = \"charting\"\n",
                "+++\n",
                "\n# Wayfind session\n\n",
                "Initiative 3 has no tickets yet. Chart the map with `wayfind ticket create`.\n",
            )
        );
    }

    #[test]
    fn nothing_available_is_an_answer_rather_than_a_failure() {
        let model = NextView {
            initiative_id: initiative_id(),
            state: InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked),
        };
        let rendered = render_next_unavailable(&model);
        assert!(rendered.contains("kind = \"next\"\ninitiative_id = 3\nstatus = \"blocked\"\n"));
        assert!(rendered.contains("# No available ticket\n\n"));
        assert!(rendered.ends_with("Run `wayfind tree`.\n"));
    }

    // -- attachments -------------------------------------------------------

    #[test]
    fn an_attachment_heading_ends_at_the_rule_before_the_document() {
        let model = AttachmentView {
            id: attachment_id(1),
            name: "notes.md".to_string(),
            ticket_id: ticket_id(4),
            bytes: 2048,
            created_at: moment(),
            description: "The old design".to_string(),
        };
        assert_eq!(
            render_attachment_header(&model),
            concat!(
                "+++\n",
                "kind = \"attachment\"\n",
                "id = 1\n",
                "name = \"notes.md\"\n",
                "ticket_id = 4\n",
                "bytes = 2048\n",
                "created_at = \"2026-08-02 13:45:09\"\n",
                "+++\n",
                "\n# notes.md\n\nThe old design\n\n---\n\n",
            )
        );
    }

    // -- search ------------------------------------------------------------

    #[test]
    fn search_reports_what_was_asked_and_what_was_found() {
        let model = SearchView {
            query: "cache".to_string(),
            limit: 10,
            offset: 0,
            hits: vec![SearchHit {
                ticket_id: ticket_id(4),
                title: "Pick a store".to_string(),
                status: TicketStatusLabel::Resolved,
                snippet: "a **cache** that\nloads".to_string(),
                metadata: Default::default(),
            }],
        };
        let rendered = render_search(&model);
        assert!(rendered.contains("query = \"cache\"\nlimit = 10\noffset = 0\n"));
        assert!(rendered.contains("- [4] Pick a store (resolved) — a **cache** that loads\n"));
    }

    // -- records and one-line reports --------------------------------------

    #[test]
    fn csv_records_carry_a_header_and_survive_awkward_text() {
        let rows = vec![DumpRow {
            id: ticket_id(4),
            title: "Pick a store, \"today\"".to_string(),
            ticket_type: TicketType::Grilling,
            status: TicketStatusLabel::Resolved,
            question: "Which\nstore?".to_string(),
            resolution: String::new(),
        }];
        let rendered = render_csv(&rows).unwrap();

        let mut reader = csv::Reader::from_reader(rendered.as_bytes());
        let headers: Vec<String> = reader
            .headers()
            .unwrap()
            .iter()
            .map(str::to_string)
            .collect();
        assert_eq!(
            headers,
            vec!["id", "title", "type", "status", "question", "resolution"]
        );
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(&record[0], "4");
        assert_eq!(&record[1], "Pick a store, \"today\"");
        assert_eq!(&record[2], "grilling");
        assert_eq!(&record[3], "resolved");
        assert_eq!(&record[4], "Which store?");
        assert_eq!(&record[5], "");
    }

    #[test]
    fn no_records_still_leaves_a_header_to_read() {
        let rendered = render_csv(&[]).unwrap();
        assert_eq!(rendered, "id,title,type,status,question,resolution\n");
    }

    #[test]
    fn the_one_line_reports_say_what_happened() {
        let key = ProjectKey::new("/Users/example/project").unwrap();
        assert_eq!(
            render_init("/Users/example/.config/wayfind/wayfind.sqlite", &key),
            "initialized /Users/example/.config/wayfind/wayfind.sqlite for /Users/example/project\n"
        );
        assert_eq!(
            render_initiative_cleared(initiative_id()),
            "initiative 3 is clear\n"
        );
    }
}
