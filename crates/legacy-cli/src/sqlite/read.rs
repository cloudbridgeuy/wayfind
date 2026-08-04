//! Reading records out of SQLite.
//!
//! Every query names its columns and aliases them to exactly what the parsers in
//! [`super::row`] ask for. That is more typing than `SELECT *`, and it is what
//! makes a schema change fail at the first query rather than at a parser that
//! quietly read the wrong column.
//!
//! Both consistency levels are served by the same query. A single SQLite file
//! read inside one process observes every committed write, so
//! [`Consistency::Strong`] costs nothing extra here. The parameter is still
//! carried across the boundary because a backend that spans machines cannot make
//! that promise for free, and the caller has to be able to ask.

use rusqlite::{Connection, OptionalExtension};
use wayfind_v1_core::{
    AttachmentId, AttachmentMetadata, AttachmentReference, Consistency, Decision, Dependency,
    EntityReader, FogNote, Initiative, InitiativeId, InitiativeRevision, InitiativeScope,
    InitiativeSelector, Project, ProjectKey, ScopeExclusion, Session, SessionId, StorageError,
    StorageResult, Ticket, TicketId,
};

use super::{failed, read_revision, row, SqliteStorage};

// ---------------------------------------------------------------------------
// Column lists
// ---------------------------------------------------------------------------

/// A ticket and the one claim that is still live, if any.
///
/// The join condition carries `released_at IS NULL`. Without it a settled
/// ticket would still report the session that once held it, because the claim
/// row outlives the claim.
const TICKET_SELECT: &str = "\
SELECT t.id AS id, t.initiative_id AS initiative_id, t.title AS title, t.type AS type,
       t.status AS status, t.question AS question, t.resolution AS resolution,
       t.created_at AS created_at, t.resolved_at AS resolved_at, t.amended_at AS amended_at,
       c.session_id AS claim_session_id, c.claimed_at AS claimed_at
FROM tickets t
LEFT JOIN ticket_claims c ON c.ticket_id = t.id AND c.released_at IS NULL";

const INITIATIVE_SELECT: &str = "\
SELECT id AS id, project_key AS project_key, name AS name, destination AS destination,
       notes AS notes, status AS status, created_at AS created_at
FROM initiatives";

const SESSION_SELECT: &str = "\
SELECT id AS id, project_key AS project_key, initiative_id AS initiative_id,
       current_ticket_id AS current_ticket_id,
       resolved_non_research_count AS resolved_non_research_count,
       status AS status, started_at AS started_at, last_seen_at AS last_seen_at
FROM sessions";

const ATTACHMENT_SELECT: &str = "\
SELECT a.id AS id, a.ticket_id AS ticket_id, a.name AS name, a.description AS description,
       a.byte_size AS byte_size, a.session_id AS session_id, a.created_at AS created_at
FROM attachments a";

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

/// Run a query that answers with many rows and parse each one.
///
/// The double result is unavoidable and worth keeping visible: the outer one is
/// SQLite failing to answer, the inner one is a row the domain refuses. They are
/// different problems and they are reported differently.
fn collect<T>(
    connection: &Connection,
    operation: &'static str,
    sql: &str,
    parameters: impl rusqlite::Params,
    parse: fn(&rusqlite::Row<'_>) -> StorageResult<T>,
) -> StorageResult<Vec<T>> {
    let mut statement = connection.prepare(sql).map_err(failed(operation))?;
    let rows = statement
        .query_map(parameters, |row| Ok(parse(row)))
        .map_err(failed(operation))?
        .collect::<rusqlite::Result<Vec<StorageResult<T>>>>()
        .map_err(failed(operation))?;
    rows.into_iter().collect()
}

/// Run a query that answers with at most one row and parse it.
///
/// "No row" is an ordinary answer here, not a failure: a project that has never
/// been recorded is simply absent.
fn one<T>(
    connection: &Connection,
    operation: &'static str,
    sql: &str,
    parameters: impl rusqlite::Params,
    parse: fn(&rusqlite::Row<'_>) -> StorageResult<T>,
) -> StorageResult<Option<T>> {
    connection
        .query_row(sql, parameters, |row| Ok(parse(row)))
        .optional()
        .map_err(failed(operation))?
        .transpose()
}

impl EntityReader for SqliteStorage {
    fn project(
        &self,
        key: &ProjectKey,
        _consistency: Consistency,
    ) -> StorageResult<Option<Project>> {
        one(
            self.connection(),
            "read project",
            "SELECT key AS key, created_at AS created_at FROM projects WHERE key = ?1;",
            [key.as_str()],
            row::parse_project,
        )
    }

    fn newest_initiative(
        &self,
        selector: InitiativeSelector<'_>,
        _consistency: Consistency,
    ) -> StorageResult<Option<InitiativeId>> {
        // Newest means highest identifier, not latest timestamp. Two initiatives
        // created in the same second must still order, and the identifier is the
        // only column that always does.
        let sql = match selector.scope {
            InitiativeScope::ExcludingClear => {
                "SELECT id FROM initiatives WHERE project_key = ?1 AND status != 'clear' \
                 ORDER BY id DESC LIMIT 1;"
            }
            InitiativeScope::AnyStatus => {
                "SELECT id FROM initiatives WHERE project_key = ?1 ORDER BY id DESC LIMIT 1;"
            }
        };
        one(
            self.connection(),
            "read newest initiative",
            sql,
            [selector.project_key.as_str()],
            |row| {
                let raw: i64 = row.get(0).map_err(failed("read newest initiative"))?;
                InitiativeId::new(raw).map_err(StorageError::CorruptData)
            },
        )
    }

    fn initiative_revision(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<InitiativeRevision> {
        read_revision(self.connection(), id)
    }

    fn initiative(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Option<Initiative>> {
        one(
            self.connection(),
            "read initiative",
            &format!("{INITIATIVE_SELECT} WHERE id = ?1;"),
            [id.get()],
            row::parse_initiative,
        )
    }

    fn tickets(&self, id: InitiativeId, _consistency: Consistency) -> StorageResult<Vec<Ticket>> {
        collect(
            self.connection(),
            "read tickets",
            &format!("{TICKET_SELECT} WHERE t.initiative_id = ?1 ORDER BY t.id;"),
            [id.get()],
            row::parse_ticket,
        )
    }

    fn dependencies(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Vec<Dependency>> {
        // Both ends are constrained to the initiative. An edge that points out of
        // it would be corrupt, and leaving it out here is the safer reading:
        // a drawing that omits an impossible edge beats one that invents a node.
        collect(
            self.connection(),
            "read dependencies",
            "SELECT d.ticket_id AS ticket_id, d.blocker_id AS blocker_id \
             FROM ticket_dependencies d \
             JOIN tickets t ON t.id = d.ticket_id \
             JOIN tickets b ON b.id = d.blocker_id \
             WHERE t.initiative_id = ?1 AND b.initiative_id = ?1 \
             ORDER BY d.ticket_id, d.blocker_id;",
            [id.get()],
            row::parse_dependency,
        )
    }

    fn sessions(&self, id: InitiativeId, _consistency: Consistency) -> StorageResult<Vec<Session>> {
        collect(
            self.connection(),
            "read sessions",
            &format!("{SESSION_SELECT} WHERE initiative_id = ?1 ORDER BY id;"),
            [id.get()],
            row::parse_session,
        )
    }

    fn decisions(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Vec<Decision>> {
        collect(
            self.connection(),
            "read decisions",
            "SELECT d.id AS id, d.ticket_id AS ticket_id, d.gist AS gist, \
                    d.created_at AS created_at \
             FROM decisions d JOIN tickets t ON t.id = d.ticket_id \
             WHERE t.initiative_id = ?1 ORDER BY d.id;",
            [id.get()],
            row::parse_decision,
        )
    }

    fn fog_notes(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Vec<FogNote>> {
        collect(
            self.connection(),
            "read fog notes",
            "SELECT id AS id, initiative_id AS initiative_id, note AS note, \
                    created_at AS created_at \
             FROM fog_notes WHERE initiative_id = ?1 ORDER BY id;",
            [id.get()],
            row::parse_fog_note,
        )
    }

    fn scope_exclusions(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Vec<ScopeExclusion>> {
        collect(
            self.connection(),
            "read scope exclusions",
            "SELECT id AS id, initiative_id AS initiative_id, note AS note, \
                    created_at AS created_at \
             FROM scope_exclusions WHERE initiative_id = ?1 ORDER BY id;",
            [id.get()],
            row::parse_scope_exclusion,
        )
    }

    fn attachment_index(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Vec<AttachmentMetadata>> {
        // The `content` column is not in `ATTACHMENT_SELECT`, so listing an
        // initiative's documents never loads their bytes.
        collect(
            self.connection(),
            "read attachment index",
            &format!(
                "{ATTACHMENT_SELECT} JOIN tickets t ON t.id = a.ticket_id \
                 WHERE t.initiative_id = ?1 ORDER BY a.id;"
            ),
            [id.get()],
            row::parse_attachment_metadata,
        )
    }

    fn attachment_references(
        &self,
        id: InitiativeId,
        _consistency: Consistency,
    ) -> StorageResult<Vec<AttachmentReference>> {
        collect(
            self.connection(),
            "read attachment references",
            "SELECT r.attachment_id AS attachment_id, r.ticket_id AS ticket_id, \
                    r.created_at AS created_at \
             FROM attachment_references r JOIN tickets t ON t.id = r.ticket_id \
             WHERE t.initiative_id = ?1 ORDER BY r.attachment_id, r.ticket_id;",
            [id.get()],
            row::parse_attachment_reference,
        )
    }

    fn ticket(&self, id: TicketId, _consistency: Consistency) -> StorageResult<Option<Ticket>> {
        one(
            self.connection(),
            "read ticket",
            &format!("{TICKET_SELECT} WHERE t.id = ?1;"),
            [id.get()],
            row::parse_ticket,
        )
    }

    fn session(&self, id: &SessionId, _consistency: Consistency) -> StorageResult<Option<Session>> {
        one(
            self.connection(),
            "read session",
            &format!("{SESSION_SELECT} WHERE id = ?1;"),
            [id.as_str()],
            row::parse_session,
        )
    }

    fn attachment_metadata(
        &self,
        id: AttachmentId,
        _consistency: Consistency,
    ) -> StorageResult<Option<AttachmentMetadata>> {
        one(
            self.connection(),
            "read attachment metadata",
            &format!("{ATTACHMENT_SELECT} WHERE a.id = ?1;"),
            [id.get()],
            row::parse_attachment_metadata,
        )
    }
}
