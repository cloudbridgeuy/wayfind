//! The storage boundary.
//!
//! Storage is five small capabilities rather than one large interface, because
//! the parts have different costs and different failure modes. A backend that
//! cannot allocate identifiers cheaply, or cannot run a five-table transaction,
//! fails to implement one trait instead of silently doing something slow or
//! wrong inside a method nobody expected to be expensive.
//!
//! Three rules hold everywhere in this module.
//!
//! **Conflicts are values, not errors.** A ticket someone else already claimed
//! is an ordinary outcome the caller must handle; it is reported through the
//! outcome types in [`crate::outcome`], never through [`StorageError`].
//! [`StorageError`] is reserved for the three things a caller cannot plan
//! around: the store being unreachable, the store holding impossible data, and
//! the store refusing a request that exceeds a capacity limit.
//!
//! **Identifiers are allocated, never discovered.** No write returns an
//! identifier as a side effect of inserting. The shell asks [`IdAllocator`] for
//! an identifier first and names it in the command, so a retried command writes
//! the same row instead of a second one.
//!
//! **Every stable-view mutation carries the revision it expects.** The shell
//! reads an initiative at a revision, decides, and submits the decision with
//! that revision attached. If the initiative moved in between, the command is
//! refused as a conflict rather than applied to a state nobody looked at.

use crate::command::{
    AddAttachmentReference, AddFogNote, AddScopeExclusion, AmendTicket, ClaimTicket,
    ClearInitiative, CreateInitiative, CreateTicket, EnsureProject, InsertDependency,
    RemoveAttachmentReference, ResolveTicket, StoreAttachment, TouchSession,
};
use crate::error::Error;
use crate::id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
use crate::model::{
    AttachmentMetadata, AttachmentReference, Decision, Dependency, FogNote, Initiative, Project,
    ScopeExclusion, Session, Ticket,
};
use crate::outcome::{
    AmendOutcome, ClaimOutcome, ClearOutcome, InsertDependencyOutcome, ReferenceOutcome,
    RemoveAttachmentOutcome, ResolveOutcome, TouchSessionOutcome,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The most items one atomic workflow may touch.
///
/// SQLite has no such limit, but a future DynamoDB adapter would: a
/// `TransactWriteItems` call takes at most 100 items totalling at most 4 MB.
/// The limit is declared here so the shell never builds a command that only one
/// backend can accept.
pub const MAX_ITEMS_PER_WORKFLOW: usize = 100;

/// The most bytes one atomic workflow may write.
///
/// See [`MAX_ITEMS_PER_WORKFLOW`] for why this limit exists in a SQLite-only
/// world.
pub const MAX_BYTES_PER_WORKFLOW: usize = 4 * 1024 * 1024;

/// A limit the store refused to exceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapacityLimit {
    /// One workflow named more participants than a backend can commit at once.
    ItemsPerWorkflow {
        /// The ceiling, which is [`MAX_ITEMS_PER_WORKFLOW`].
        limit: usize,
        /// How many the command asked for.
        requested: usize,
    },
    /// One workflow carried more bytes than a backend can commit at once.
    BytesPerWorkflow {
        /// The ceiling, which is [`MAX_BYTES_PER_WORKFLOW`].
        limit: usize,
        /// How many the command asked for.
        requested: usize,
    },
    /// One value was larger than the store accepts.
    ValueTooLarge {
        /// Which value was too large.
        field: &'static str,
        /// The ceiling in bytes.
        limit: usize,
        /// The actual size in bytes.
        actual: usize,
    },
}

impl std::fmt::Display for CapacityLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapacityLimit::ItemsPerWorkflow { limit, requested } => write!(
                f,
                "a single workflow may touch at most {limit} items, but this one names {requested}"
            ),
            CapacityLimit::BytesPerWorkflow { limit, requested } => write!(
                f,
                "a single workflow may write at most {limit} bytes, but this one carries {requested}"
            ),
            CapacityLimit::ValueTooLarge {
                field,
                limit,
                actual,
            } => write!(
                f,
                "{field} may be at most {limit} bytes, but this one is {actual}"
            ),
        }
    }
}

/// The three failures a caller cannot plan around.
///
/// Everything a caller *can* plan around — a ticket already claimed, a
/// dependency that would close a cycle, an attachment that is not there — is an
/// outcome value instead. See [`crate::outcome`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    /// The store could not be reached, or refused the request for a reason the
    /// domain has no opinion about.
    #[error("storage failed during {operation}: {detail}")]
    Infrastructure {
        /// The operation that failed, for the operator's benefit.
        operation: &'static str,
        /// What the backend reported.
        detail: String,
    },
    /// The store returned a record the domain says cannot exist.
    #[error("{0}")]
    CorruptData(#[from] Error),
    /// The request exceeded a declared capacity limit.
    #[error("storage capacity limit: {0}")]
    CapacityExceeded(CapacityLimit),
}

impl StorageError {
    /// Build an [`StorageError::Infrastructure`] without repeating the syntax.
    pub fn infrastructure(operation: &'static str, detail: impl Into<String>) -> Self {
        StorageError::Infrastructure {
            operation,
            detail: detail.into(),
        }
    }
}

/// The result type every storage capability returns.
pub type StorageResult<T> = std::result::Result<T, StorageError>;

// ---------------------------------------------------------------------------
// Consistency and revisions
// ---------------------------------------------------------------------------

/// How fresh a read has to be.
///
/// A single-process SQLite adapter satisfies both levels with the same query. A
/// distributed backend would pay for [`Consistency::Strong`], so the caller
/// says which one it needs instead of the adapter guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Consistency {
    /// The read must observe every write that committed before it started.
    ///
    /// Required for anything a decision is made from.
    Strong,
    /// The read may lag. Acceptable for display-only listings.
    Relaxed,
}

/// A counter that changes whenever an initiative's stable view changes.
///
/// "Stable view" means the records `map`, `next`, and `handoff` read: the
/// initiative row, its tickets, its dependencies, its decisions, its notes, and
/// its attachment index. Allocating an identifier and touching a session's
/// `last_seen_at` do not change what any of those reads report, so neither one
/// moves the revision.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct InitiativeRevision(u64);

impl InitiativeRevision {
    /// The revision an initiative starts at.
    pub const INITIAL: InitiativeRevision = InitiativeRevision(0);

    /// Wrap a raw counter read out of a store.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw counter, for an adapter to store.
    pub fn get(self) -> u64 {
        self.0
    }

    /// The revision that follows this one.
    ///
    /// Saturating rather than wrapping: a revision that stops moving refuses
    /// every optimistic write, which is safe. A revision that wraps would start
    /// accepting stale ones.
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl std::fmt::Display for InitiativeRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Identifier allocation
// ---------------------------------------------------------------------------

/// Which counter to draw the next identifier from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdScope {
    /// `initiatives.id`
    Initiative,
    /// `tickets.id`
    Ticket,
    /// `attachments.id`
    Attachment,
    /// `decisions.id`
    Decision,
    /// `fog_notes.id`
    FogNote,
    /// `scope_exclusions.id`
    ScopeExclusion,
}

impl IdScope {
    /// The scope's name, used in errors.
    pub fn as_str(self) -> &'static str {
        match self {
            IdScope::Initiative => "initiative",
            IdScope::Ticket => "ticket",
            IdScope::Attachment => "attachment",
            IdScope::Decision => "decision",
            IdScope::FogNote => "fog note",
            IdScope::ScopeExclusion => "scope exclusion",
        }
    }
}

/// A freshly allocated identifier, tagged with the scope it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocatedId {
    /// An initiative identifier.
    Initiative(InitiativeId),
    /// A ticket identifier.
    Ticket(TicketId),
    /// An attachment identifier.
    Attachment(AttachmentId),
    /// A decision identifier.
    Decision(DecisionId),
    /// A fog note or scope exclusion identifier.
    Note(NoteId),
}

/// Build the accessor that unwraps one [`AllocatedId`] variant.
macro_rules! allocated_id_accessor {
    ($method:ident, $variant:ident, $ty:ty, $label:literal) => {
        #[doc = concat!("The identifier, if this allocation came from the ", $label, " scope.")]
        pub fn $method(self) -> StorageResult<$ty> {
            match self {
                AllocatedId::$variant(id) => Ok(id),
                other => Err(StorageError::infrastructure(
                    "allocate",
                    format!(
                        concat!(
                            "asked for ",
                            $label,
                            " identifier but the allocator returned {:?}"
                        ),
                        other
                    ),
                )),
            }
        }
    };
}

impl AllocatedId {
    allocated_id_accessor!(initiative, Initiative, InitiativeId, "initiative");
    allocated_id_accessor!(ticket, Ticket, TicketId, "ticket");
    allocated_id_accessor!(attachment, Attachment, AttachmentId, "attachment");
    allocated_id_accessor!(decision, Decision, DecisionId, "decision");
    allocated_id_accessor!(note, Note, NoteId, "note");
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Which initiative of a project a read means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitiativeSelector<'a> {
    /// The project to look in.
    pub project_key: &'a ProjectKey,
    /// Whether a cleared initiative counts.
    pub scope: InitiativeScope,
}

/// Whether a cleared initiative is a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitiativeScope {
    /// Only initiatives that are still open. This is what a write follows.
    ExcludingClear,
    /// Any initiative, cleared or not. This is what a read falls back to, so a
    /// cleared map stays readable for the handoff.
    AnyStatus,
}

/// Reading entities out of the store.
pub trait EntityReader {
    /// Look a project up by its key.
    fn project(&self, key: &ProjectKey, consistency: Consistency)
        -> StorageResult<Option<Project>>;

    /// The newest initiative matching the selector.
    fn newest_initiative(
        &self,
        selector: InitiativeSelector<'_>,
        consistency: Consistency,
    ) -> StorageResult<Option<InitiativeId>>;

    /// The initiative's current revision.
    fn initiative_revision(
        &self,
        id: InitiativeId,
        consistency: Consistency,
    ) -> StorageResult<InitiativeRevision>;

    /// One initiative.
    fn initiative(
        &self,
        id: InitiativeId,
        consistency: Consistency,
    ) -> StorageResult<Option<Initiative>>;

    /// Every ticket of an initiative, ordered by identifier.
    fn tickets(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<Ticket>>;

    /// Every dependency edge of an initiative.
    fn dependencies(
        &self,
        id: InitiativeId,
        consistency: Consistency,
    ) -> StorageResult<Vec<Dependency>>;

    /// Every session bound to an initiative.
    fn sessions(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<Session>>;

    /// Every decision of an initiative, in decision order.
    fn decisions(&self, id: InitiativeId, consistency: Consistency)
        -> StorageResult<Vec<Decision>>;

    /// Every "not yet specified" note of an initiative, in insertion order.
    fn fog_notes(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<FogNote>>;

    /// Every "out of scope" note of an initiative, in insertion order.
    fn scope_exclusions(
        &self,
        id: InitiativeId,
        consistency: Consistency,
    ) -> StorageResult<Vec<ScopeExclusion>>;

    /// Every attachment of an initiative, without its bytes.
    fn attachment_index(
        &self,
        id: InitiativeId,
        consistency: Consistency,
    ) -> StorageResult<Vec<AttachmentMetadata>>;

    /// Every cross-ticket attachment reference inside an initiative.
    fn attachment_references(
        &self,
        id: InitiativeId,
        consistency: Consistency,
    ) -> StorageResult<Vec<AttachmentReference>>;

    /// One ticket, wherever it lives.
    fn ticket(&self, id: TicketId, consistency: Consistency) -> StorageResult<Option<Ticket>>;

    /// One session, wherever it lives.
    fn session(&self, id: &SessionId, consistency: Consistency) -> StorageResult<Option<Session>>;

    /// One attachment's metadata, wherever it lives.
    fn attachment_metadata(
        &self,
        id: AttachmentId,
        consistency: Consistency,
    ) -> StorageResult<Option<AttachmentMetadata>>;
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// The writes that touch one entity and need no cross-entity transaction.
pub trait EntityWriter {
    /// Record a project if it is not recorded yet. Idempotent.
    fn ensure_project(&self, command: EnsureProject) -> StorageResult<Project>;

    /// Create an initiative at an identifier the caller allocated.
    fn create_initiative(&self, command: CreateInitiative) -> StorageResult<Initiative>;

    /// Create a ticket at an identifier the caller allocated.
    fn create_ticket(&self, command: CreateTicket) -> StorageResult<Ticket>;

    /// Repair the recorded text of a resolved ticket.
    fn amend_ticket(&self, command: AmendTicket) -> StorageResult<AmendOutcome>;

    /// Mark an initiative clear.
    fn clear_initiative(&self, command: ClearInitiative) -> StorageResult<ClearOutcome>;

    /// Record that a session is alive, creating it if this is its first command.
    ///
    /// This does not move the initiative revision: `last_seen_at` changes
    /// nothing any stable read reports.
    fn touch_session(&self, command: TouchSession) -> StorageResult<TouchSessionOutcome>;

    /// Add a "not yet specified" note.
    fn add_fog_note(&self, command: AddFogNote) -> StorageResult<FogNote>;

    /// Add an "out of scope" note.
    fn add_scope_exclusion(&self, command: AddScopeExclusion) -> StorageResult<ScopeExclusion>;
}

/// Handing out identifiers.
///
/// The promise is monotonic and unique, with gaps allowed. A caller that
/// allocates and then fails has burned an identifier; nothing reuses it.
pub trait IdAllocator {
    /// Take the next identifier from a scope.
    fn allocate(&self, scope: IdScope) -> StorageResult<AllocatedId>;
}

/// The three workflows that must commit as one unit or not at all.
///
/// Each command names every participant it touches, the state or revision it
/// expects each participant to be in, and the clock reading to write. A backend
/// applies all of it under one transaction, or reports which expectation failed.
pub trait AtomicWorkflows {
    /// Take a ticket for a session.
    ///
    /// Covers the ticket, the claim row, the session, and the initiative
    /// revision.
    fn claim_ticket(&self, command: ClaimTicket) -> StorageResult<ClaimOutcome>;

    /// Record a decision and release the session.
    ///
    /// Covers the ticket, the claim row, the session, the new decision, and the
    /// initiative revision.
    fn resolve_ticket(&self, command: ResolveTicket) -> StorageResult<ResolveOutcome>;

    /// Add one dependency edge.
    fn insert_dependency(
        &self,
        command: InsertDependency,
    ) -> StorageResult<InsertDependencyOutcome>;
}

/// Storing attachment documents.
///
/// Bytes cross this boundary as opaque `[u8]`. A backend may store them as
/// text, as a blob, or beside the record entirely; nothing above this trait
/// depends on which.
pub trait AttachmentStore {
    /// Store one document at an identifier the caller allocated.
    fn store_attachment(
        &self,
        command: StoreAttachment,
        bytes: &[u8],
    ) -> StorageResult<AttachmentMetadata>;

    /// Read one document's bytes back.
    fn read_attachment(&self, id: AttachmentId) -> StorageResult<Option<Vec<u8>>>;

    /// Delete one document and every reference to it.
    fn remove_attachment(&self, id: AttachmentId) -> StorageResult<RemoveAttachmentOutcome>;

    /// Point a ticket at an attachment another ticket owns. Idempotent.
    fn add_reference(&self, command: AddAttachmentReference) -> StorageResult<ReferenceOutcome>;

    /// Drop such a pointer. Idempotent.
    fn remove_reference(
        &self,
        command: RemoveAttachmentReference,
    ) -> StorageResult<ReferenceOutcome>;
}

/// Everything Wayfind needs from a store.
pub trait Storage:
    EntityReader + EntityWriter + IdAllocator + AtomicWorkflows + AttachmentStore
{
}

impl<T> Storage for T where
    T: EntityReader + EntityWriter + IdAllocator + AtomicWorkflows + AttachmentStore
{
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        AllocatedId, CapacityLimit, Consistency, IdScope, InitiativeRevision, Storage,
        StorageError, MAX_BYTES_PER_WORKFLOW, MAX_ITEMS_PER_WORKFLOW,
    };
    use crate::id::TicketId;

    /// Compiles only while [`Storage`] stays object safe.
    fn accepts_any_storage(_storage: &dyn Storage) {}

    #[test]
    fn storage_is_object_safe() {
        let _: fn(&dyn Storage) = accepts_any_storage;
    }

    #[test]
    fn revisions_start_at_zero_and_move_forward() {
        let first = InitiativeRevision::INITIAL;
        assert_eq!(first.get(), 0);
        assert_eq!(first.next().get(), 1);
        assert!(first < first.next());
    }

    #[test]
    fn revisions_saturate_rather_than_wrap() {
        let last = InitiativeRevision::new(u64::MAX);
        assert_eq!(last.next(), last);
    }

    #[test]
    fn allocated_ids_unwrap_only_their_own_scope() {
        let allocated = AllocatedId::Ticket(TicketId::new(7).unwrap());
        assert_eq!(allocated.ticket().unwrap(), TicketId::new(7).unwrap());
        assert!(allocated.decision().is_err());
    }

    #[test]
    fn id_scopes_name_themselves() {
        assert_eq!(IdScope::FogNote.as_str(), "fog note");
        assert_eq!(IdScope::Ticket.as_str(), "ticket");
    }

    #[test]
    fn consistency_levels_are_distinct() {
        assert_ne!(Consistency::Strong, Consistency::Relaxed);
    }

    #[test]
    fn capacity_limits_explain_themselves() {
        let limit = CapacityLimit::ItemsPerWorkflow {
            limit: MAX_ITEMS_PER_WORKFLOW,
            requested: 101,
        };
        assert_eq!(
            StorageError::CapacityExceeded(limit).to_string(),
            "storage capacity limit: a single workflow may touch at most 100 items, but this one names 101"
        );
    }

    #[test]
    fn the_declared_workflow_limits_match_the_tightest_backend() {
        assert_eq!(MAX_ITEMS_PER_WORKFLOW, 100);
        assert_eq!(MAX_BYTES_PER_WORKFLOW, 4 * 1024 * 1024);
    }

    #[test]
    fn infrastructure_errors_name_the_operation() {
        let error = StorageError::infrastructure("claim_ticket", "database is locked");
        assert_eq!(
            error.to_string(),
            "storage failed during claim_ticket: database is locked"
        );
    }
}
