//! The three workflows that must move several records at once.
//!
//! Claiming, resolving, and adding a dependency each touch records that are only
//! meaningful together. A claim that wrote the ticket but not the session would
//! leave a session free to take a second ticket while still holding the first —
//! exactly the state the whole design exists to prevent. So each one runs inside
//! a single transaction, and each one starts by confirming the initiative has not
//! moved since the core decided.
//!
//! Every refusal here is a value, not an error. "Another session holds it" is an
//! ordinary answer to an ordinary question, and the caller has to be able to
//! match on it and say something useful. Only a broken database or a broken
//! SQLite call raises [`StorageError`](wayfind_core::StorageError).

use rusqlite::{Connection, OptionalExtension};
use wayfind_core::{
    AtomicWorkflows, ClaimConflict, ClaimOutcome, ClaimTicket, InsertDependency,
    InsertDependencyConflict, InsertDependencyOutcome, ResolveConflict, ResolveOutcome,
    ResolveTicket, StorageResult, TicketState,
};

use super::write::{initiative_of_ticket, read_session, read_ticket};
use super::{advance_revision, failed, revision_conflict, SqliteStorage};

/// Whether a session has already spent its one non-research resolution.
///
/// The script tests this at both ends — before a claim and again before a
/// resolution — so a session cannot take a second non-research ticket and only
/// discover the rule when it tries to settle it.
fn budget_spent(count: u32) -> bool {
    count > 0
}

impl AtomicWorkflows for SqliteStorage {
    fn claim_ticket(&self, command: ClaimTicket) -> StorageResult<ClaimOutcome> {
        let transaction = self.write_transaction()?;
        if let Some(stale) = revision_conflict(
            &transaction,
            command.initiative_id,
            command.expected_initiative_revision,
        )? {
            return Ok(ClaimOutcome::Conflict(ClaimConflict::StaleRevision(stale)));
        }

        let Some(ticket) = read_ticket(&transaction, "claim ticket", command.ticket_id)? else {
            return Ok(ClaimOutcome::Conflict(ClaimConflict::NoSuchTicket {
                ticket_id: command.ticket_id,
            }));
        };
        if ticket.initiative_id != command.initiative_id {
            return Ok(ClaimOutcome::Conflict(
                ClaimConflict::TicketOutsideInitiative {
                    ticket_id: command.ticket_id,
                },
            ));
        }

        // The command carries what the core decided against; the checks below
        // read what is actually stored. Where the two differ the stored value
        // wins, because the conflict it produces names the session an operator
        // has to go and talk to.
        match &ticket.state {
            TicketState::Open => {}
            TicketState::Claimed { claimant, .. } if *claimant == command.session_id => {
                // Re-claiming a ticket this session already holds changes
                // nothing, and is not a mistake worth an error.
                return Ok(ClaimOutcome::AlreadyHeld);
            }
            TicketState::Claimed { claimant, .. } => {
                return Ok(ClaimOutcome::Conflict(ClaimConflict::AlreadyClaimed {
                    ticket_id: command.ticket_id,
                    claimant: claimant.clone(),
                }));
            }
            other => {
                return Ok(ClaimOutcome::Conflict(ClaimConflict::TicketNotOpen {
                    ticket_id: command.ticket_id,
                    status: other.label(),
                }));
            }
        }

        if let Some(session) = read_session(&transaction, "claim ticket", &command.session_id)? {
            if let Some(held) = session.state.held_ticket() {
                if held != command.ticket_id {
                    return Ok(ClaimOutcome::Conflict(
                        ClaimConflict::SessionHoldsAnotherTicket { held },
                    ));
                }
            }
            if !ticket.ticket_type.is_research()
                && budget_spent(session.resolved_non_research_count)
            {
                return Ok(ClaimOutcome::Conflict(
                    ClaimConflict::NonResearchBudgetSpent {
                        ticket_id: command.ticket_id,
                    },
                ));
            }
        }

        let now = command.now.to_storage_string();
        // `INSERT OR REPLACE` rather than `INSERT`: a ticket claimed, released,
        // and claimed again reuses one row, because `ticket_claims.ticket_id` is
        // the primary key. The old row is history nobody reads.
        transaction
            .execute(
                "INSERT OR REPLACE INTO ticket_claims(ticket_id, session_id, claimed_at, \
                                                      released_at) \
                 VALUES (?1, ?2, ?3, NULL);",
                rusqlite::params![command.ticket_id.get(), command.session_id.as_str(), now],
            )
            .map_err(failed("claim ticket"))?;
        transaction
            .execute(
                "UPDATE tickets SET status = 'claimed' WHERE id = ?1;",
                [command.ticket_id.get()],
            )
            .map_err(failed("claim ticket"))?;
        transaction
            .execute(
                "UPDATE sessions SET current_ticket_id = ?1, last_seen_at = ?2 WHERE id = ?3;",
                rusqlite::params![command.ticket_id.get(), now, command.session_id.as_str()],
            )
            .map_err(failed("claim ticket"))?;
        advance_revision(&transaction, command.initiative_id)?;
        transaction.commit().map_err(failed("claim ticket"))?;
        Ok(ClaimOutcome::Claimed)
    }

    fn resolve_ticket(&self, command: ResolveTicket) -> StorageResult<ResolveOutcome> {
        let transaction = self.write_transaction()?;
        if let Some(stale) = revision_conflict(
            &transaction,
            command.initiative_id,
            command.expected_initiative_revision,
        )? {
            return Ok(ResolveOutcome::Conflict(ResolveConflict::StaleRevision(
                stale,
            )));
        }

        let Some(ticket) = read_ticket(&transaction, "resolve ticket", command.ticket_id)? else {
            return Ok(ResolveOutcome::Conflict(ResolveConflict::NoSuchTicket {
                ticket_id: command.ticket_id,
            }));
        };
        if ticket.initiative_id != command.initiative_id {
            return Ok(ResolveOutcome::Conflict(
                ResolveConflict::TicketOutsideInitiative {
                    ticket_id: command.ticket_id,
                },
            ));
        }

        // A settled ticket is checked for first. It is not claimed either, and
        // "already resolved" is the more useful of the two answers.
        match &ticket.state {
            TicketState::Resolved { .. } => {
                return Ok(ResolveOutcome::Conflict(ResolveConflict::AlreadyResolved {
                    ticket_id: command.ticket_id,
                }));
            }
            TicketState::Claimed { claimant, .. } if *claimant != command.session_id => {
                return Ok(ResolveOutcome::Conflict(
                    ResolveConflict::ClaimedByAnotherSession {
                        ticket_id: command.ticket_id,
                        claimant: claimant.clone(),
                    },
                ));
            }
            TicketState::Claimed { .. } => {}
            _ => {
                return Ok(ResolveOutcome::Conflict(ResolveConflict::NotClaimed {
                    ticket_id: command.ticket_id,
                }));
            }
        }

        let session = read_session(&transaction, "resolve ticket", &command.session_id)?;
        let spends = command.spends_non_research_budget();
        if let Some(session) = &session {
            if spends && budget_spent(session.resolved_non_research_count) {
                return Ok(ResolveOutcome::Conflict(
                    ResolveConflict::NonResearchBudgetSpent {
                        ticket_id: command.ticket_id,
                    },
                ));
            }
        }

        let now = command.now.to_storage_string();
        transaction
            .execute(
                "UPDATE tickets SET status = 'resolved', resolution = ?1, resolved_at = ?2 \
                 WHERE id = ?3;",
                rusqlite::params![command.resolution.as_str(), now, command.ticket_id.get()],
            )
            .map_err(failed("resolve ticket"))?;
        transaction
            .execute(
                "UPDATE ticket_claims SET released_at = ?1 \
                 WHERE ticket_id = ?2 AND released_at IS NULL;",
                rusqlite::params![now, command.ticket_id.get()],
            )
            .map_err(failed("resolve ticket"))?;
        // The counter is raised in SQL rather than read, added to, and written
        // back, so the arithmetic happens where the row lock already is.
        transaction
            .execute(
                "UPDATE sessions SET current_ticket_id = NULL, \
                        resolved_non_research_count = resolved_non_research_count + ?1, \
                        last_seen_at = ?2 \
                 WHERE id = ?3;",
                rusqlite::params![i64::from(spends), now, command.session_id.as_str()],
            )
            .map_err(failed("resolve ticket"))?;
        transaction
            .execute(
                "INSERT INTO decisions(id, ticket_id, gist, created_at) VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![
                    command.decision_id.get(),
                    command.ticket_id.get(),
                    command.resolution.gist(),
                    now
                ],
            )
            .map_err(failed("resolve ticket"))?;
        advance_revision(&transaction, command.initiative_id)?;
        transaction.commit().map_err(failed("resolve ticket"))?;
        Ok(ResolveOutcome::Resolved)
    }

    fn insert_dependency(
        &self,
        command: InsertDependency,
    ) -> StorageResult<InsertDependencyOutcome> {
        let transaction = self.write_transaction()?;
        if let Some(stale) = revision_conflict(
            &transaction,
            command.initiative_id,
            command.expected_initiative_revision,
        )? {
            return Ok(InsertDependencyOutcome::Conflict(
                InsertDependencyConflict::StaleRevision(stale),
            ));
        }

        // The schema carries a `CHECK(ticket_id != blocker_id)`, and hitting it
        // would raise an opaque constraint failure. Naming the case here keeps
        // the answer a value.
        if command.ticket_id == command.blocker_id {
            return Ok(InsertDependencyOutcome::Conflict(
                InsertDependencyConflict::SelfEdge {
                    ticket_id: command.ticket_id,
                },
            ));
        }

        for ticket_id in [command.ticket_id, command.blocker_id] {
            match initiative_of_ticket(&transaction, "insert dependency", ticket_id)? {
                None => {
                    return Ok(InsertDependencyOutcome::Conflict(
                        InsertDependencyConflict::NoSuchTicket { ticket_id },
                    ));
                }
                Some(initiative_id) if initiative_id != command.initiative_id => {
                    return Ok(InsertDependencyOutcome::Conflict(
                        InsertDependencyConflict::TicketOutsideInitiative { ticket_id },
                    ));
                }
                Some(_) => {}
            }
        }

        // Whether the edge would close a cycle was decided in the core, against
        // the graph read at `expected_initiative_revision`. The revision match
        // above is what says that graph is still current, so the walk is not
        // repeated here.
        if edge_exists(&transaction, &command)? {
            return Ok(InsertDependencyOutcome::AlreadyPresent);
        }

        transaction
            .execute(
                "INSERT INTO ticket_dependencies(ticket_id, blocker_id) VALUES (?1, ?2);",
                rusqlite::params![command.ticket_id.get(), command.blocker_id.get()],
            )
            .map_err(failed("insert dependency"))?;
        advance_revision(&transaction, command.initiative_id)?;
        transaction.commit().map_err(failed("insert dependency"))?;
        Ok(InsertDependencyOutcome::Inserted)
    }
}

/// Whether this exact edge is already recorded.
fn edge_exists(connection: &Connection, command: &InsertDependency) -> StorageResult<bool> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM ticket_dependencies WHERE ticket_id = ?1 AND blocker_id = ?2;",
            rusqlite::params![command.ticket_id.get(), command.blocker_id.get()],
            |row| row.get(0),
        )
        .optional()
        .map_err(failed("insert dependency"))?;
    Ok(found.is_some())
}
