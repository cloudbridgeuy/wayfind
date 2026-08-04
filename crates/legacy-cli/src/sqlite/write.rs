//! Writing single records, and handing out identifiers.
//!
//! Two capabilities live here because they share one rule: neither of them ever
//! discovers an identifier. A row is written at an identifier the caller already
//! reserved, so the same command applied twice writes the same row rather than a
//! second one.
//!
//! Everything that changes what `map`, `next`, or `handoff` would print also
//! moves the initiative's revision, inside the same transaction as the change.
//! Recording a session's heartbeat and reserving an identifier do not, because
//! neither changes anything those views report.

use rusqlite::{Connection, OptionalExtension};
use wayfind_v1_core::{
    AddFogNote, AddScopeExclusion, AllocatedId, AmendConflict, AmendOutcome, AmendTicket,
    AttachmentId, ClearConflict, ClearInitiative, ClearOutcome, CreateInitiative, CreateTicket,
    DecisionId, EnsureProject, EntityWriter, FogNote, IdAllocator, IdScope, Initiative,
    InitiativeId, NoteId, PersistedInitiativeStatus, Project, ScopeExclusion, SessionId,
    StorageError, StorageResult, Ticket, TicketId, TicketState, TouchSession, TouchSessionConflict,
    TouchSessionOutcome,
};

use super::{advance_revision, failed, revision_conflict, row, SqliteStorage};

// ---------------------------------------------------------------------------
// Identifier allocation
// ---------------------------------------------------------------------------

/// The table each scope draws its identifiers from.
///
/// The counter alone is not enough. A database the script wrote has rows and no
/// counter, so the first allocation in each scope has to look at what is already
/// there. After that the counter carries it, which is what keeps identifiers
/// moving forward even when rows are deleted.
fn scope_table(scope: IdScope) -> &'static str {
    match scope {
        IdScope::Initiative => "initiatives",
        IdScope::Ticket => "tickets",
        IdScope::Attachment => "attachments",
        IdScope::Decision => "decisions",
        IdScope::FogNote => "fog_notes",
        IdScope::ScopeExclusion => "scope_exclusions",
    }
}

/// Wrap a raw identifier in the variant its scope calls for.
fn tag(scope: IdScope, raw: i64) -> StorageResult<AllocatedId> {
    let allocated = match scope {
        IdScope::Initiative => AllocatedId::Initiative(InitiativeId::new(raw)?),
        IdScope::Ticket => AllocatedId::Ticket(TicketId::new(raw)?),
        IdScope::Attachment => AllocatedId::Attachment(AttachmentId::new(raw)?),
        IdScope::Decision => AllocatedId::Decision(DecisionId::new(raw)?),
        IdScope::FogNote | IdScope::ScopeExclusion => AllocatedId::Note(NoteId::new(raw)?),
    };
    Ok(allocated)
}

impl IdAllocator for SqliteStorage {
    fn allocate(&self, scope: IdScope) -> StorageResult<AllocatedId> {
        let transaction = self.write_transaction()?;
        let table = scope_table(scope);

        let counter: Option<i64> = transaction
            .query_row(
                "SELECT next_id FROM wayfind_id_sequences WHERE scope = ?1;",
                [scope.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(failed("allocate identifier"))?;
        let highest: i64 = transaction
            .query_row(
                &format!("SELECT coalesce(max(id), 0) FROM {table};"),
                [],
                |row| row.get(0),
            )
            .map_err(failed("allocate identifier"))?;

        let next = counter
            .unwrap_or(0)
            .max(highest)
            .checked_add(1)
            .ok_or_else(|| {
                StorageError::infrastructure(
                    "allocate identifier",
                    format!("the {} counter has no room left", scope.as_str()),
                )
            })?;
        transaction
            .execute(
                "INSERT INTO wayfind_id_sequences(scope, next_id) VALUES (?1, ?2) \
                 ON CONFLICT(scope) DO UPDATE SET next_id = excluded.next_id;",
                rusqlite::params![scope.as_str(), next],
            )
            .map_err(failed("allocate identifier"))?;

        let allocated = tag(scope, next)?;
        transaction
            .commit()
            .map_err(failed("allocate identifier"))?;
        Ok(allocated)
    }
}

// ---------------------------------------------------------------------------
// Single-record writes
// ---------------------------------------------------------------------------

/// Read back the row a write just made.
///
/// A write returns the stored record rather than the command's own fields, so
/// what the caller renders is what the database holds — including any default
/// the schema filled in.
fn reread<T>(
    connection: &Connection,
    operation: &'static str,
    sql: &str,
    parameters: impl rusqlite::Params,
    parse: fn(&rusqlite::Row<'_>) -> StorageResult<T>,
) -> StorageResult<T> {
    connection
        .query_row(sql, parameters, |row| Ok(parse(row)))
        .map_err(failed(operation))?
}

/// Explain a uniqueness failure in the operator's terms.
///
/// SQLite says `UNIQUE constraint failed: initiatives.project_key, name`, which
/// is true and useless. The trait has no conflict value for a duplicate name, so
/// the next best thing is an error that says what to do about it.
fn duplicate(
    operation: &'static str,
    explanation: &'static str,
) -> impl Fn(rusqlite::Error) -> StorageError {
    move |error| {
        let text = error.to_string();
        if text.contains("UNIQUE constraint failed") {
            return StorageError::infrastructure(operation, explanation);
        }
        StorageError::infrastructure(operation, text)
    }
}

impl EntityWriter for SqliteStorage {
    fn ensure_project(&self, command: EnsureProject) -> StorageResult<Project> {
        let connection = self.connection();
        connection
            .execute(
                "INSERT INTO projects(key, created_at) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO NOTHING;",
                rusqlite::params![command.key.as_str(), command.now.to_storage_string()],
            )
            .map_err(failed("record project"))?;
        reread(
            connection,
            "record project",
            "SELECT key AS key, created_at AS created_at FROM projects WHERE key = ?1;",
            [command.key.as_str()],
            row::parse_project,
        )
    }

    fn create_initiative(&self, command: CreateInitiative) -> StorageResult<Initiative> {
        let transaction = self.write_transaction()?;
        transaction
            .execute(
                "INSERT INTO initiatives(id, project_key, name, destination, notes, status, \
                                         created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'charting', ?6);",
                rusqlite::params![
                    command.id.get(),
                    command.project_key.as_str(),
                    command.name.as_str(),
                    command.destination.as_str(),
                    command.notes,
                    command.now.to_storage_string(),
                ],
            )
            .map_err(duplicate(
                "create initiative",
                "this project already has an initiative with that name",
            ))?;
        let created = reread(
            &transaction,
            "create initiative",
            "SELECT id AS id, project_key AS project_key, name AS name, \
                    destination AS destination, notes AS notes, status AS status, \
                    created_at AS created_at FROM initiatives WHERE id = ?1;",
            [command.id.get()],
            row::parse_initiative,
        )?;
        transaction.commit().map_err(failed("create initiative"))?;
        Ok(created)
    }

    fn create_ticket(&self, command: CreateTicket) -> StorageResult<Ticket> {
        let transaction = self.write_transaction()?;
        transaction
            .execute(
                "INSERT INTO tickets(id, initiative_id, title, type, status, question, \
                                     created_at) \
                 VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6);",
                rusqlite::params![
                    command.id.get(),
                    command.initiative_id.get(),
                    command.title.as_str(),
                    command.ticket_type.as_str(),
                    command.question,
                    command.now.to_storage_string(),
                ],
            )
            .map_err(duplicate(
                "create ticket",
                "this initiative already has a ticket with that title",
            ))?;
        advance_revision(&transaction, command.initiative_id)?;
        let created = read_ticket(&transaction, "create ticket", command.id)?.ok_or_else(|| {
            StorageError::infrastructure(
                "create ticket",
                "the ticket vanished after it was written",
            )
        })?;
        transaction.commit().map_err(failed("create ticket"))?;
        Ok(created)
    }

    fn amend_ticket(&self, command: AmendTicket) -> StorageResult<AmendOutcome> {
        let transaction = self.write_transaction()?;
        if let Some(stale) = revision_conflict(
            &transaction,
            command.initiative_id,
            command.expected_initiative_revision,
        )? {
            return Ok(AmendOutcome::Conflict(AmendConflict::StaleRevision(stale)));
        }

        let Some(ticket) = read_ticket(&transaction, "amend ticket", command.ticket_id)? else {
            return Ok(AmendOutcome::Conflict(AmendConflict::NoSuchTicket {
                ticket_id: command.ticket_id,
            }));
        };
        if ticket.initiative_id != command.initiative_id {
            return Ok(AmendOutcome::Conflict(
                AmendConflict::TicketOutsideInitiative {
                    ticket_id: command.ticket_id,
                },
            ));
        }
        if !matches!(ticket.state, TicketState::Resolved { .. }) {
            return Ok(AmendOutcome::Conflict(AmendConflict::NotResolved {
                ticket_id: command.ticket_id,
                status: ticket.state.label(),
            }));
        }

        // The recorded text and the gist the map prints are two columns in two
        // tables. Repairing one without the other is exactly the inconsistency
        // this command exists to remove.
        transaction
            .execute(
                "UPDATE tickets SET resolution = ?1, amended_at = ?2 WHERE id = ?3;",
                rusqlite::params![
                    command.resolution.as_str(),
                    command.now.to_storage_string(),
                    command.ticket_id.get(),
                ],
            )
            .map_err(failed("amend ticket"))?;
        transaction
            .execute(
                "UPDATE decisions SET gist = ?1 WHERE ticket_id = ?2;",
                rusqlite::params![command.resolution.gist(), command.ticket_id.get()],
            )
            .map_err(failed("amend ticket"))?;
        advance_revision(&transaction, command.initiative_id)?;
        transaction.commit().map_err(failed("amend ticket"))?;
        Ok(AmendOutcome::Amended)
    }

    fn clear_initiative(&self, command: ClearInitiative) -> StorageResult<ClearOutcome> {
        let transaction = self.write_transaction()?;
        if let Some(stale) = revision_conflict(
            &transaction,
            command.initiative_id,
            command.expected_initiative_revision,
        )? {
            return Ok(ClearOutcome::Conflict(ClearConflict::StaleRevision(stale)));
        }

        let status: Option<String> = transaction
            .query_row(
                "SELECT status FROM initiatives WHERE id = ?1;",
                [command.initiative_id.get()],
                |row| row.get(0),
            )
            .optional()
            .map_err(failed("clear initiative"))?;
        let Some(status) = status else {
            return Ok(ClearOutcome::Conflict(ClearConflict::NoSuchInitiative));
        };
        let status: PersistedInitiativeStatus = status.parse()?;
        if status.is_clear() {
            return Ok(ClearOutcome::AlreadyClear);
        }

        let outstanding: i64 = transaction
            .query_row(
                "SELECT count(*) FROM tickets WHERE initiative_id = ?1 \
                 AND status IN ('open', 'claimed');",
                [command.initiative_id.get()],
                |row| row.get(0),
            )
            .map_err(failed("clear initiative"))?;
        if outstanding > 0 {
            let outstanding = u32::try_from(outstanding).unwrap_or(u32::MAX);
            return Ok(ClearOutcome::Conflict(ClearConflict::OpenTicketsRemain {
                outstanding,
            }));
        }

        transaction
            .execute(
                "UPDATE initiatives SET status = 'clear' WHERE id = ?1;",
                [command.initiative_id.get()],
            )
            .map_err(failed("clear initiative"))?;
        advance_revision(&transaction, command.initiative_id)?;
        transaction.commit().map_err(failed("clear initiative"))?;
        Ok(ClearOutcome::Cleared)
    }

    fn touch_session(&self, command: TouchSession) -> StorageResult<TouchSessionOutcome> {
        let transaction = self.write_transaction()?;
        let existing = read_session(&transaction, "record session", &command.session_id)?;
        let now = command.now.to_storage_string();

        // A session never moves. It names one project and one initiative for its
        // whole life, so a session identifier reused somewhere else is refused
        // rather than quietly re-pointed.
        let started = match &existing {
            Some(session) => {
                if session.project_key != command.project_key
                    || session.initiative_id != command.initiative_id
                {
                    return Ok(TouchSessionOutcome::Conflict(
                        TouchSessionConflict::SessionBoundElsewhere {
                            owner_project: session.project_key.clone(),
                            owner_initiative: session.initiative_id,
                        },
                    ));
                }
                false
            }
            None => {
                transaction
                    .execute(
                        "INSERT INTO sessions(id, project_key, initiative_id, \
                                              current_ticket_id, resolved_non_research_count, \
                                              status, started_at, last_seen_at) \
                         VALUES (?1, ?2, ?3, NULL, 0, 'active', ?4, ?4);",
                        rusqlite::params![
                            command.session_id.as_str(),
                            command.project_key.as_str(),
                            command.initiative_id.map(InitiativeId::get),
                            now,
                        ],
                    )
                    .map_err(failed("record session"))?;
                true
            }
        };

        transaction
            .execute(
                "UPDATE sessions SET last_seen_at = ?1 WHERE id = ?2;",
                rusqlite::params![now, command.session_id.as_str()],
            )
            .map_err(failed("record session"))?;

        let session = read_session(&transaction, "record session", &command.session_id)?
            .ok_or_else(|| {
                StorageError::infrastructure(
                    "record session",
                    "the session vanished after it was written",
                )
            })?;
        transaction.commit().map_err(failed("record session"))?;

        // A heartbeat deliberately does not move the revision: `last_seen_at`
        // changes nothing that `map`, `next`, or `handoff` report.
        Ok(if started {
            TouchSessionOutcome::Started(session)
        } else {
            TouchSessionOutcome::Refreshed(session)
        })
    }

    fn add_fog_note(&self, command: AddFogNote) -> StorageResult<FogNote> {
        let transaction = self.write_transaction()?;
        transaction
            .execute(
                "INSERT INTO fog_notes(id, initiative_id, note, created_at) \
                 VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![
                    command.id.get(),
                    command.initiative_id.get(),
                    command.note.as_str(),
                    command.now.to_storage_string(),
                ],
            )
            .map_err(failed("add fog note"))?;
        advance_revision(&transaction, command.initiative_id)?;
        let created = reread(
            &transaction,
            "add fog note",
            "SELECT id AS id, initiative_id AS initiative_id, note AS note, \
                    created_at AS created_at FROM fog_notes WHERE id = ?1;",
            [command.id.get()],
            row::parse_fog_note,
        )?;
        transaction.commit().map_err(failed("add fog note"))?;
        Ok(created)
    }

    fn add_scope_exclusion(&self, command: AddScopeExclusion) -> StorageResult<ScopeExclusion> {
        let transaction = self.write_transaction()?;
        transaction
            .execute(
                "INSERT INTO scope_exclusions(id, initiative_id, note, created_at) \
                 VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![
                    command.id.get(),
                    command.initiative_id.get(),
                    command.note.as_str(),
                    command.now.to_storage_string(),
                ],
            )
            .map_err(failed("add scope exclusion"))?;
        advance_revision(&transaction, command.initiative_id)?;
        let created = reread(
            &transaction,
            "add scope exclusion",
            "SELECT id AS id, initiative_id AS initiative_id, note AS note, \
                    created_at AS created_at FROM scope_exclusions WHERE id = ?1;",
            [command.id.get()],
            row::parse_scope_exclusion,
        )?;
        transaction
            .commit()
            .map_err(failed("add scope exclusion"))?;
        Ok(created)
    }
}

// ---------------------------------------------------------------------------
// Shared inside-a-transaction reads
// ---------------------------------------------------------------------------

/// One ticket with its live claim, read inside whatever transaction is running.
pub(crate) fn read_ticket(
    connection: &Connection,
    operation: &'static str,
    id: TicketId,
) -> StorageResult<Option<Ticket>> {
    connection
        .query_row(
            "SELECT t.id AS id, t.initiative_id AS initiative_id, t.title AS title, \
                    t.type AS type, t.status AS status, t.question AS question, \
                    t.resolution AS resolution, t.created_at AS created_at, \
                    t.resolved_at AS resolved_at, t.amended_at AS amended_at, \
                    c.session_id AS claim_session_id, c.claimed_at AS claimed_at \
             FROM tickets t \
             LEFT JOIN ticket_claims c ON c.ticket_id = t.id AND c.released_at IS NULL \
             WHERE t.id = ?1;",
            [id.get()],
            |row| Ok(row::parse_ticket(row)),
        )
        .optional()
        .map_err(failed(operation))?
        .transpose()
}

/// One session, read inside whatever transaction is running.
pub(crate) fn read_session(
    connection: &Connection,
    operation: &'static str,
    id: &SessionId,
) -> StorageResult<Option<wayfind_v1_core::Session>> {
    connection
        .query_row(
            "SELECT id AS id, project_key AS project_key, initiative_id AS initiative_id, \
                    current_ticket_id AS current_ticket_id, \
                    resolved_non_research_count AS resolved_non_research_count, \
                    status AS status, started_at AS started_at, last_seen_at AS last_seen_at \
             FROM sessions WHERE id = ?1;",
            [id.as_str()],
            |row| Ok(row::parse_session(row)),
        )
        .optional()
        .map_err(failed(operation))?
        .transpose()
}

/// The initiative a ticket belongs to, if the ticket is there at all.
pub(crate) fn initiative_of_ticket(
    connection: &Connection,
    operation: &'static str,
    id: TicketId,
) -> StorageResult<Option<InitiativeId>> {
    connection
        .query_row(
            "SELECT initiative_id FROM tickets WHERE id = ?1;",
            [id.get()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(failed(operation))?
        .map(|raw| InitiativeId::new(raw).map_err(StorageError::CorruptData))
        .transpose()
}
