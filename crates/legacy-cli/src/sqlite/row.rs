//! Turning stored rows into domain values.
//!
//! A row is a bag of nullable columns; a domain value is not. This module is
//! the one place the two meet, and it is strict on purpose: a combination the
//! domain says cannot exist — a claimed ticket with nobody holding it, a closed
//! session still holding work — is reported as [`StorageError::CorruptData`]
//! rather than smoothed over into something plausible.
//!
//! Reading columns by name rather than by position is deliberate. A query that
//! grows a column in the middle then stays correct, and a mismatch between a
//! parser and its query fails loudly instead of silently reading the wrong
//! value.

use rusqlite::types::FromSql;
use rusqlite::Row;
use wayfind_v1_core::{
    AttachmentId, AttachmentMetadata, AttachmentReference, Decision, DecisionId, Dependency,
    FogNote, Initiative, InitiativeId, NoteId, PersistedClaim, PersistedInitiativeStatus,
    PersistedSessionState, PersistedTicketState, Project, ProjectKey, ScopeExclusion, Session,
    SessionId, SessionState, StorageError, StorageResult, Ticket, TicketId, TicketState,
    TicketStatusLabel, TicketType, Timestamp,
};

// ---------------------------------------------------------------------------
// Column readers
// ---------------------------------------------------------------------------

/// Read one column by name.
fn column<T: FromSql>(row: &Row<'_>, name: &str) -> StorageResult<T> {
    row.get(name).map_err(|error| {
        StorageError::infrastructure("read column", format!("column `{name}`: {error}"))
    })
}

/// Read a text column that the schema declares `NOT NULL`.
fn text(row: &Row<'_>, name: &str) -> StorageResult<String> {
    column(row, name)
}

/// Read a text column that may be null, treating empty text as absent.
///
/// The script's SQLite invocations printed a null as the empty string, so the
/// two are already indistinguishable in the data the script wrote. Treating
/// them the same here keeps a row written by either program reading alike.
fn optional_text(row: &Row<'_>, name: &str) -> StorageResult<Option<String>> {
    let value: Option<String> = column(row, name)?;
    Ok(value.filter(|text| !text.is_empty()))
}

/// Read a whole number that must not be negative.
fn count(row: &Row<'_>, name: &str) -> StorageResult<u64> {
    let value: i64 = column(row, name)?;
    u64::try_from(value).map_err(|_| {
        corrupt(
            "record",
            format!("column `{name}` holds a negative count: {value}"),
        )
    })
}

/// Read a timestamp column the schema declares `NOT NULL`.
fn timestamp(row: &Row<'_>, name: &str) -> StorageResult<Timestamp> {
    let raw = text(row, name)?;
    raw.parse().map_err(|error: wayfind_v1_core::Error| {
        corrupt("record", format!("column `{name}`: {error}"))
    })
}

/// Build a corrupt-data error without repeating the syntax.
fn corrupt(entity: &'static str, reason: impl Into<String>) -> StorageError {
    StorageError::CorruptData(wayfind_v1_core::Error::corrupt_data(entity, reason))
}

/// Re-label a core parse failure as corrupt stored data.
///
/// A value that fails to parse on its way *out* of the store is never the
/// caller's mistake, so it must not surface as an invalid argument.
fn parsed<T>(result: wayfind_v1_core::Result<T>, name: &str) -> StorageResult<T> {
    result.map_err(|error| corrupt("record", format!("column `{name}`: {error}")))
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Read an identifier column the schema declares `NOT NULL`.
fn initiative_id(row: &Row<'_>, name: &str) -> StorageResult<InitiativeId> {
    parsed(InitiativeId::new(column(row, name)?), name)
}

fn ticket_id(row: &Row<'_>, name: &str) -> StorageResult<TicketId> {
    parsed(TicketId::new(column(row, name)?), name)
}

fn attachment_id(row: &Row<'_>, name: &str) -> StorageResult<AttachmentId> {
    parsed(AttachmentId::new(column(row, name)?), name)
}

fn decision_id(row: &Row<'_>, name: &str) -> StorageResult<DecisionId> {
    parsed(DecisionId::new(column(row, name)?), name)
}

fn note_id(row: &Row<'_>, name: &str) -> StorageResult<NoteId> {
    parsed(NoteId::new(column(row, name)?), name)
}

fn project_key(row: &Row<'_>, name: &str) -> StorageResult<ProjectKey> {
    parsed(ProjectKey::new(text(row, name)?), name)
}

fn session_id(row: &Row<'_>, name: &str) -> StorageResult<SessionId> {
    parsed(SessionId::new(text(row, name)?), name)
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// Parse a `projects` row.
///
/// Columns: `key`, `created_at`.
pub fn parse_project(row: &Row<'_>) -> StorageResult<Project> {
    Ok(Project {
        key: project_key(row, "key")?,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse an `initiatives` row.
///
/// Columns: `id`, `project_key`, `name`, `destination`, `notes`, `status`,
/// `created_at`.
pub fn parse_initiative(row: &Row<'_>) -> StorageResult<Initiative> {
    let status = text(row, "status")?;
    Ok(Initiative {
        id: initiative_id(row, "id")?,
        project_key: project_key(row, "project_key")?,
        name: text(row, "name")?,
        destination: text(row, "destination")?,
        notes: text(row, "notes")?,
        status: parsed(status.parse::<PersistedInitiativeStatus>(), "status")?,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse a `tickets` row joined to its live claim.
///
/// Columns: `id`, `initiative_id`, `title`, `type`, `status`, `question`,
/// `resolution`, `created_at`, `resolved_at`, `amended_at`, and the two joined
/// claim columns `claim_session_id` and `claimed_at`, both null unless an
/// unreleased claim exists.
///
/// The join has to be on `released_at IS NULL`. A released claim row stays in
/// the table forever, and treating one as live would report a resolved ticket
/// as still held.
pub fn parse_ticket(row: &Row<'_>) -> StorageResult<Ticket> {
    let status = text(row, "status")?;
    let resolution = optional_text(row, "resolution")?;
    let resolved_at = optional_text(row, "resolved_at")?;
    let amended_at = optional_text(row, "amended_at")?;
    let claim_session = optional_text(row, "claim_session_id")?;
    let claimed_at = optional_text(row, "claimed_at")?;

    let live_claim = match (&claim_session, &claimed_at) {
        (Some(session), Some(at)) => Some(PersistedClaim {
            session_id: session,
            claimed_at: at,
        }),
        (None, None) => None,
        _ => {
            return Err(corrupt(
                "ticket",
                "a claim row names a session without a time, or a time without a session",
            ))
        }
    };

    let state = parsed(
        TicketState::from_persisted(PersistedTicketState {
            status: &status,
            resolution: resolution.as_deref(),
            resolved_at: resolved_at.as_deref(),
            amended_at: amended_at.as_deref(),
            live_claim,
        }),
        "status",
    )?;

    Ok(Ticket {
        id: ticket_id(row, "id")?,
        initiative_id: initiative_id(row, "initiative_id")?,
        title: text(row, "title")?,
        ticket_type: parsed(text(row, "type")?.parse::<TicketType>(), "type")?,
        question: text(row, "question")?,
        state,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse the status word alone, for the index views that print nothing else.
///
/// Column: `status`.
pub fn parse_ticket_status(row: &Row<'_>) -> StorageResult<TicketStatusLabel> {
    parsed(text(row, "status")?.parse::<TicketStatusLabel>(), "status")
}

/// Parse a `ticket_dependencies` row.
///
/// Columns: `ticket_id`, `blocker_id`.
///
/// The table's own `CHECK` forbids a self edge, so one appearing here means the
/// constraint was bypassed and the row is corrupt.
pub fn parse_dependency(row: &Row<'_>) -> StorageResult<Dependency> {
    let ticket = ticket_id(row, "ticket_id")?;
    let blocker = ticket_id(row, "blocker_id")?;
    Dependency::new(ticket, blocker)
        .map_err(|error| corrupt("dependency", format!("stored edge is impossible: {error}")))
}

/// Parse a `sessions` row.
///
/// Columns: `id`, `project_key`, `initiative_id`, `current_ticket_id`,
/// `resolved_non_research_count`, `status`, `started_at`, `last_seen_at`.
pub fn parse_session(row: &Row<'_>) -> StorageResult<Session> {
    let status = text(row, "status")?;
    let current: Option<i64> = column(row, "current_ticket_id")?;
    let state = parsed(
        SessionState::from_persisted(PersistedSessionState {
            status: &status,
            current_ticket_id: current,
        }),
        "status",
    )?;

    let initiative: Option<i64> = column(row, "initiative_id")?;
    let initiative_id = initiative
        .map(|raw| parsed(InitiativeId::new(raw), "initiative_id"))
        .transpose()?;

    let resolved = count(row, "resolved_non_research_count")?;
    let resolved_non_research_count = u32::try_from(resolved).map_err(|_| {
        corrupt(
            "session",
            format!("resolved_non_research_count is implausibly large: {resolved}"),
        )
    })?;

    Ok(Session {
        id: session_id(row, "id")?,
        project_key: project_key(row, "project_key")?,
        initiative_id,
        state,
        resolved_non_research_count,
        started_at: timestamp(row, "started_at")?,
        last_seen_at: timestamp(row, "last_seen_at")?,
    })
}

/// Parse a `decisions` row.
///
/// Columns: `id`, `ticket_id`, `gist`, `created_at`.
pub fn parse_decision(row: &Row<'_>) -> StorageResult<Decision> {
    Ok(Decision {
        id: decision_id(row, "id")?,
        ticket_id: ticket_id(row, "ticket_id")?,
        gist: text(row, "gist")?,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse a `fog_notes` row.
///
/// Columns: `id`, `initiative_id`, `note`, `created_at`.
pub fn parse_fog_note(row: &Row<'_>) -> StorageResult<FogNote> {
    Ok(FogNote {
        id: note_id(row, "id")?,
        initiative_id: initiative_id(row, "initiative_id")?,
        note: text(row, "note")?,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse a `scope_exclusions` row.
///
/// Columns: `id`, `initiative_id`, `note`, `created_at`.
pub fn parse_scope_exclusion(row: &Row<'_>) -> StorageResult<ScopeExclusion> {
    Ok(ScopeExclusion {
        id: note_id(row, "id")?,
        initiative_id: initiative_id(row, "initiative_id")?,
        note: text(row, "note")?,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse an `attachments` row, without its bytes.
///
/// Columns: `id`, `ticket_id`, `name`, `description`, `byte_size`,
/// `session_id`, `created_at`. The `content` column is deliberately absent:
/// listing attachments must not load a megabyte per row.
pub fn parse_attachment_metadata(row: &Row<'_>) -> StorageResult<AttachmentMetadata> {
    let session = optional_text(row, "session_id")?
        .map(|raw| parsed(SessionId::new(raw), "session_id"))
        .transpose()?;
    Ok(AttachmentMetadata {
        id: attachment_id(row, "id")?,
        ticket_id: ticket_id(row, "ticket_id")?,
        name: text(row, "name")?,
        description: text(row, "description")?,
        byte_size: count(row, "byte_size")?,
        session_id: session,
        created_at: timestamp(row, "created_at")?,
    })
}

/// Parse an `attachment_references` row.
///
/// Columns: `attachment_id`, `ticket_id`, `created_at`.
pub fn parse_attachment_reference(row: &Row<'_>) -> StorageResult<AttachmentReference> {
    Ok(AttachmentReference {
        attachment_id: attachment_id(row, "attachment_id")?,
        ticket_id: ticket_id(row, "ticket_id")?,
        created_at: timestamp(row, "created_at")?,
    })
}
