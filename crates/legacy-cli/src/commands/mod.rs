//! The command handlers, and the small amount of state they share.
//!
//! Every handler has the same shape: read what the store holds, ask
//! `wayfind_v1_core` what that means, apply whatever command the core returned,
//! then render. No handler decides who may claim a ticket, whether an edge
//! would close a cycle, or what a map should say — it only carries values
//! between the store, the core, and the output.
//!
//! [`Shell`] holds the four things nearly every handler needs: the store, the
//! search index, the environment, and the identity of the caller. It also holds
//! the two ways an initiative is chosen, because that choice is the one piece
//! of lifecycle behaviour the script kept outside its commands, and it must be
//! kept in exactly one place here too.

pub mod attachment;
pub mod initiative;
pub mod query;
pub mod session;
pub mod ticket;

use wayfind_v1_core::{
    active_sessions, prepare_touch_session, read_stable_initiative, AmendConflict, AttachmentRow,
    ClaimConflict, ClearConflict, Consistency, EnsureProject, EntityReader, Error, FrontierRow,
    Initiative, InitiativeHeader, InitiativeId, InitiativeScope, InitiativeSelector,
    InitiativeView, InsertDependencyConflict, MapView, ProjectKey, ReadPolicy, ReferenceConflict,
    ReferencedAttachmentRow, ResolveConflict, SearchBackend, Session, SessionId, SessionRow,
    StableRead, StaleRevision, Storage, Ticket, TicketId, TicketView, Timestamp, TouchInput,
    TouchSessionConflict, TouchSessionOutcome,
};

use crate::context::{self, Environment};
use crate::error::{ShellError, ShellResult};

/// Everything a handler is given.
pub struct Shell<'a> {
    /// The store, as capabilities rather than as SQL.
    pub storage: &'a dyn Storage,
    /// The full-text index.
    pub search: &'a dyn SearchBackend,
    /// The machine the program is running on.
    pub environment: &'a dyn Environment,
    /// The tree the operator is working in.
    pub project: ProjectKey,
    /// A session named on the command line, if one was.
    pub chosen_session: Option<String>,
    /// An initiative named on the command line, if one was.
    pub chosen_initiative: Option<i64>,
    /// One clock reading, taken before the command ran and used throughout, so
    /// that everything one command writes carries one time.
    pub now: Timestamp,
}

impl Shell<'_> {
    /// The store seen as a reader, for the core functions that only read.
    fn reader(&self) -> &dyn EntityReader {
        self.storage
    }

    /// Who is asking.
    pub fn session(&self) -> ShellResult<SessionId> {
        context::session_id(self.environment, self.chosen_session.as_deref())
    }

    /// Check that an initiative named on the command line belongs to this
    /// project.
    ///
    /// An initiative from another tree is refused rather than acted on, however
    /// deliberately it was named: `--initiative` chooses among this project's
    /// maps, it does not reach across projects.
    fn chosen_in_project(&self, raw: i64) -> ShellResult<InitiativeId> {
        let id = InitiativeId::new(raw)?;
        match self.storage.initiative(id, Consistency::Strong)? {
            Some(initiative) if initiative.project_key == self.project => Ok(id),
            _ => Err(ShellError::refused(format!(
                "initiative {id} is not in this project"
            ))),
        }
    }

    /// The newest initiative of this project, optionally counting cleared ones.
    fn newest(&self, scope: InitiativeScope) -> ShellResult<Option<InitiativeId>> {
        Ok(self.storage.newest_initiative(
            InitiativeSelector {
                project_key: &self.project,
                scope,
            },
            Consistency::Strong,
        )?)
    }

    /// The initiative a command that writes should write to.
    ///
    /// A cleared map is finished, so it is never picked up by implication. When
    /// the only map is a cleared one the refusal says both ways forward,
    /// because reopening finished work has to be a decision somebody made.
    pub fn writable_initiative(&self) -> ShellResult<InitiativeId> {
        if let Some(raw) = self.chosen_initiative {
            return self.chosen_in_project(raw);
        }
        if let Some(id) = self.newest(InitiativeScope::ExcludingClear)? {
            return Ok(id);
        }
        match self.newest(InitiativeScope::AnyStatus)? {
            Some(newest) => Err(ShellError::refused(format!(
                "no active initiative for this project; initiative {newest} is clear — \
                 run initiative create for new work, or pass --initiative {newest} \
                 to change it deliberately"
            ))),
            None => Err(ShellError::refused(
                "no initiative for this project; run initiative create",
            )),
        }
    }

    /// The initiative a command that only reads should report on.
    ///
    /// Reading falls back to a cleared map. Somebody asking for the handoff of
    /// work that just finished should get it, not a refusal.
    pub fn readable_initiative(&self) -> ShellResult<InitiativeId> {
        if let Some(raw) = self.chosen_initiative {
            return self.chosen_in_project(raw);
        }
        if let Some(id) = self.newest(InitiativeScope::ExcludingClear)? {
            return Ok(id);
        }
        match self.newest(InitiativeScope::AnyStatus)? {
            Some(id) => Ok(id),
            None => Err(ShellError::refused(
                "no initiative for this project; run initiative create",
            )),
        }
    }

    /// One initiative and everything in it, read until two reads agree.
    ///
    /// A view is many queries. Between the first and the last another process
    /// can commit, and a view assembled across that commit describes a world
    /// that never existed. The core re-reads until the revision holds still.
    pub fn view(&self, id: InitiativeId) -> ShellResult<InitiativeView> {
        match read_stable_initiative(self.reader(), id, ReadPolicy::default())? {
            StableRead::Stable(view) => Ok(*view),
            StableRead::Missing => Err(ShellError::refused(format!("no initiative {id}"))),
            StableRead::Unsettled { attempts } => Err(ShellError::refused(format!(
                "initiative {id} kept changing across {attempts} reads; try again"
            ))),
        }
    }

    /// The view a writing command works from.
    pub fn writable_view(&self) -> ShellResult<InitiativeView> {
        let id = self.writable_initiative()?;
        self.view(id)
    }

    /// The view a reading command works from.
    pub fn readable_view(&self) -> ShellResult<InitiativeView> {
        let id = self.readable_initiative()?;
        self.view(id)
    }

    /// Make sure the project row exists before anything references it.
    pub fn ensure_project(&self) -> ShellResult<()> {
        self.storage.ensure_project(EnsureProject {
            key: self.project.clone(),
            now: self.now,
        })?;
        Ok(())
    }

    /// Record that this session is alive and working on this initiative.
    ///
    /// Only the four commands that act as a session do this: resuming, taking a
    /// ticket, settling one, and filing a document. Looking at a map is not
    /// participation and does not leave a mark.
    pub fn touch_session(&self, initiative_id: InitiativeId) -> ShellResult<Session> {
        let session_id = self.session()?;
        self.ensure_project()?;
        let existing = self.storage.session(&session_id, Consistency::Strong)?;
        let command = prepare_touch_session(
            TouchInput {
                session_id,
                project_key: self.project.clone(),
                initiative_id: Some(initiative_id),
                now: self.now,
            },
            existing.as_ref(),
        )
        .map_err(session_refusal)?;
        match self.storage.touch_session(command)? {
            TouchSessionOutcome::Started(session) | TouchSessionOutcome::Refreshed(session) => {
                Ok(session)
            }
            TouchSessionOutcome::Conflict(conflict) => Err(session_refusal(conflict)),
        }
    }

    /// A ticket anywhere in this project, for the commands that address a
    /// document rather than a map.
    ///
    /// `attach show` and `attach rm` name an attachment, and an attachment
    /// belongs to a ticket rather than to whichever initiative happens to be
    /// current. The project is still the fence.
    pub fn ticket_in_project(&self, id: TicketId) -> ShellResult<Ticket> {
        let outside = || ShellError::refused(format!("ticket {id} is not in this project"));
        let ticket = self
            .storage
            .ticket(id, Consistency::Strong)?
            .ok_or_else(outside)?;
        let initiative = self
            .storage
            .initiative(ticket.initiative_id, Consistency::Strong)?
            .ok_or_else(outside)?;
        if initiative.project_key != self.project {
            return Err(outside());
        }
        Ok(ticket)
    }

    /// The map document for an initiative.
    pub fn map_document(&self, view: &InitiativeView) -> ShellResult<String> {
        let id = view.id();
        let decisions = self.storage.decisions(id, Consistency::Strong)?;
        let mut rows = Vec::with_capacity(decisions.len());
        for decision in &decisions {
            let ticket = view.ticket(decision.ticket_id).ok_or_else(|| {
                Error::corrupt_data(
                    "decision",
                    format!(
                        "decision {} names ticket {}, which is not in initiative {id}",
                        decision.id, decision.ticket_id
                    ),
                )
            })?;
            // The recorded decision text wins over the gist that was clipped
            // from it, so that an amended decision shows its repair here too.
            let gist = match ticket.state.resolution() {
                Some(resolution) if !resolution.is_empty() => resolution.to_owned(),
                _ => decision.gist.clone(),
            };
            rows.push(wayfind_v1_core::DecisionRow {
                ticket_id: decision.ticket_id,
                title: ticket.title.clone(),
                gist,
            });
        }

        let model = MapView {
            initiative: header(view.initiative()),
            frontier: view
                .frontier()
                .into_iter()
                .map(|entry| FrontierRow {
                    id: entry.id,
                    title: entry.title,
                    ticket_type: entry.ticket_type,
                })
                .collect(),
            state: wayfind_v1_core::classify_initiative(view),
            decisions: rows,
            fog: self
                .storage
                .fog_notes(id, Consistency::Strong)?
                .into_iter()
                .map(|note| note.note)
                .collect(),
            exclusions: self
                .storage
                .scope_exclusions(id, Consistency::Strong)?
                .into_iter()
                .map(|note| note.note)
                .collect(),
        };
        Ok(wayfind_v1_core::render_map(&model))
    }

    /// The ticket document for one ticket of an initiative.
    pub fn ticket_document(&self, view: &InitiativeView, id: TicketId) -> ShellResult<String> {
        let ticket = view.ticket(id).ok_or_else(|| {
            ShellError::refused(format!("ticket {id} is not in initiative {}", view.id()))
        })?;

        let mut blocked_by: Vec<TicketId> = view
            .dependencies()
            .iter()
            .filter(|edge| edge.ticket_id() == id)
            .map(|edge| edge.blocker_id())
            .collect();
        blocked_by.sort_unstable();

        let owned = self
            .storage
            .attachment_index(view.id(), Consistency::Strong)?;
        let attachments = owned
            .iter()
            .filter(|document| document.ticket_id == id)
            .map(|document| AttachmentRow {
                id: document.id,
                name: document.name.clone(),
                bytes: document.byte_size,
                description: document.description.clone(),
            })
            .collect();

        let mut referenced = Vec::new();
        for reference in self
            .storage
            .attachment_references(view.id(), Consistency::Strong)?
            .iter()
            .filter(|reference| reference.ticket_id == id)
        {
            // A reference can point at a document owned by a ticket in another
            // initiative, which the index of this one does not carry.
            let Some(document) = self
                .storage
                .attachment_metadata(reference.attachment_id, Consistency::Strong)?
            else {
                continue;
            };
            referenced.push(ReferencedAttachmentRow {
                id: document.id,
                name: document.name,
                bytes: document.byte_size,
                owner: document.ticket_id,
                description: document.description,
            });
        }

        let model = TicketView {
            id: ticket.id,
            title: ticket.title.clone(),
            ticket_type: ticket.ticket_type,
            status: ticket.state.label(),
            question: ticket.question.clone(),
            resolution: ticket.state.resolution().map(str::to_owned),
            amended_at: amended_at(ticket),
            blocked_by,
            attachments,
            referenced,
        };
        Ok(wayfind_v1_core::render_ticket(&model))
    }

    /// The rows `session list` prints.
    pub fn session_rows(&self, view: &InitiativeView) -> Vec<SessionRow> {
        active_sessions(view)
            .into_iter()
            .map(|session| SessionRow {
                id: session.id.clone(),
                holding: session.state.held_ticket().map(|ticket_id| {
                    let title = view
                        .ticket(ticket_id)
                        .map(|ticket| ticket.title.clone())
                        .unwrap_or_default();
                    (ticket_id, title)
                }),
                last_seen_at: session.last_seen_at,
            })
            .collect()
    }
}

/// The header every initiative document opens with.
pub fn header(initiative: &Initiative) -> InitiativeHeader {
    InitiativeHeader {
        id: initiative.id,
        name: initiative.name.clone(),
        destination: initiative.destination.clone(),
        notes: initiative.notes.clone(),
        status: initiative.status,
    }
}

/// When a ticket's recorded decision was last repaired.
fn amended_at(ticket: &Ticket) -> Option<Timestamp> {
    match &ticket.state {
        wayfind_v1_core::TicketState::Resolved { amended_at, .. } => *amended_at,
        _ => None,
    }
}

/// Say why a session cannot be used.
fn session_refusal(conflict: TouchSessionConflict) -> ShellError {
    let TouchSessionConflict::SessionBoundElsewhere {
        owner_project,
        owner_initiative,
    } = conflict;
    let where_it_lives = match owner_initiative {
        Some(id) => format!("initiative {id} of {}", owner_project.as_str()),
        None => owner_project.as_str().to_owned(),
    };
    ShellError::refused(format!(
        "this session already belongs to {where_it_lives}; use a new session ID"
    ))
}

// ---------------------------------------------------------------------------
// Saying no
// ---------------------------------------------------------------------------
//
// A conflict is a value the core hands back, in the core's own vocabulary. The
// sentence an operator reads is a shell concern, so the translation lives here
// — once per conflict, whether the conflict came from the read-only check or
// from the store finding the world had moved underneath it.

/// A revision that no longer holds.
///
/// This one is always worth retrying, and the sentence says so, because every
/// other refusal stays true however many times it is repeated.
fn stale(stale: StaleRevision) -> String {
    format!(
        "the initiative changed while this command was deciding \
         (expected revision {}, found {}); run it again",
        stale.expected.get(),
        stale.actual.get()
    )
}

/// Say why a ticket cannot be taken.
pub fn claim_refusal(conflict: ClaimConflict) -> ShellError {
    ShellError::refused(match conflict {
        ClaimConflict::NoSuchTicket { ticket_id }
        | ClaimConflict::TicketOutsideInitiative { ticket_id } => {
            format!("ticket {ticket_id} is not in this initiative")
        }
        ClaimConflict::TicketNotOpen { ticket_id, status } => {
            format!("ticket {ticket_id} is {status}, not open")
        }
        ClaimConflict::AlreadyClaimed {
            ticket_id,
            claimant,
        } => format!(
            "ticket {ticket_id} is already claimed by session {}",
            claimant.as_str()
        ),
        ClaimConflict::SessionHoldsAnotherTicket { held } => {
            format!("this session already has active ticket {held}")
        }
        ClaimConflict::NonResearchBudgetSpent { .. } => {
            "this session already resolved a non-research ticket".to_owned()
        }
        ClaimConflict::StaleRevision(revision) => stale(revision),
    })
}

/// Say why a decision cannot be recorded.
pub fn resolve_refusal(conflict: ResolveConflict) -> ShellError {
    ShellError::refused(match conflict {
        ResolveConflict::NoSuchTicket { ticket_id }
        | ResolveConflict::TicketOutsideInitiative { ticket_id } => {
            format!("ticket {ticket_id} is not in this initiative")
        }
        ResolveConflict::NotClaimed { .. } => {
            "ticket must be claimed by this session before resolution".to_owned()
        }
        ResolveConflict::ClaimedByAnotherSession {
            ticket_id,
            claimant,
        } => format!(
            "ticket {ticket_id} is claimed by session {}, so only that session may resolve it",
            claimant.as_str()
        ),
        ResolveConflict::AlreadyResolved { ticket_id } => {
            format!("ticket {ticket_id} already carries a decision")
        }
        ResolveConflict::NonResearchBudgetSpent { .. } => {
            "this session already resolved a non-research ticket".to_owned()
        }
        ResolveConflict::StaleRevision(revision) => stale(revision),
    })
}

/// Say why a recorded decision cannot be repaired.
pub fn amend_refusal(conflict: &AmendConflict) -> ShellError {
    ShellError::refused(match conflict {
        AmendConflict::NoSuchTicket { ticket_id }
        | AmendConflict::TicketOutsideInitiative { ticket_id } => {
            format!("ticket {ticket_id} is not in this initiative")
        }
        AmendConflict::NotResolved { ticket_id, .. } => {
            format!("ticket {ticket_id} is not resolved; amend repairs a recorded decision")
        }
        AmendConflict::StaleRevision(revision) => stale(*revision),
    })
}

/// Say why one ticket cannot be made to wait on another.
pub fn dependency_refusal(conflict: InsertDependencyConflict) -> ShellError {
    ShellError::refused(match conflict {
        InsertDependencyConflict::NoSuchTicket { ticket_id }
        | InsertDependencyConflict::TicketOutsideInitiative { ticket_id } => {
            format!("ticket {ticket_id} is not in this initiative")
        }
        InsertDependencyConflict::SelfEdge { ticket_id } => {
            format!("ticket {ticket_id} cannot wait on itself")
        }
        InsertDependencyConflict::WouldCloseCycle { cycle } => {
            let path = cycle
                .iter()
                .map(|id| id.get().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            format!("dependency would create a cycle: {path}")
        }
        InsertDependencyConflict::StaleRevision(revision) => stale(revision),
    })
}

/// Say why an initiative cannot be closed.
pub fn clear_refusal(conflict: &ClearConflict) -> ShellError {
    ShellError::refused(match conflict {
        ClearConflict::NoSuchInitiative => "no such initiative".to_owned(),
        ClearConflict::OpenTicketsRemain { outstanding } => {
            format!("{outstanding} ticket(s) are still unresolved")
        }
        ClearConflict::StaleRevision(revision) => stale(*revision),
    })
}

/// Say why a reference cannot be changed.
pub fn reference_refusal(conflict: &ReferenceConflict) -> ShellError {
    ShellError::refused(match conflict {
        ReferenceConflict::NoSuchAttachment => "no such attachment".to_owned(),
        ReferenceConflict::NoSuchTicket { ticket_id } => {
            format!("ticket {ticket_id} is not in this initiative")
        }
        ReferenceConflict::TicketOwnsAttachment { ticket_id } => {
            format!("ticket {ticket_id} owns this attachment")
        }
    })
}
