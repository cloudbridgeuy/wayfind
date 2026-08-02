//! Wayfind's functional core.
//!
//! Every business rule lives here as a pure function over strict values. The
//! crate performs no input or output: it never reads a clock, a file, an
//! environment variable, or a database. The shell in `wayfind_cli` supplies
//! every effect as data and applies every decision this crate returns.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod command;
pub mod error;
pub mod graph;
pub mod id;
pub mod initiative;
pub mod model;
pub mod outcome;
pub mod search;
pub mod session;
pub mod storage;
pub mod time;
pub mod transition;

pub use command::{
    AddAttachmentReference, AddFogNote, AddScopeExclusion, AmendTicket, AttachmentName,
    ClaimTicket, ClearInitiative, CreateInitiative, CreateTicket, EnsureProject, InsertDependency,
    NonEmptyText, RemoveAttachmentReference, ResolutionText, ResolveTicket, StoreAttachment,
    TouchSession,
};
pub use error::{Error, Result};
pub use graph::{cycle_from, frontier, would_create_cycle, DependencyGraph};
pub use id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
pub use initiative::{
    classify_initiative, next_ticket, read_stable_initiative, InitiativeView, ReadPolicy,
    StableRead, TicketCounts,
};
pub use model::{
    ActiveSessionState, AttachmentMetadata, AttachmentReference, BlockedReason, Decision,
    Dependency, FogNote, FrontierTicket, Initiative, InitiativeState, NonEmptyVec, PersistedClaim,
    PersistedInitiativeStatus, PersistedSessionState, PersistedTicketState, Project,
    ScopeExclusion, Session, SessionState, Ticket, TicketState, TicketStatusLabel, TicketType,
};
pub use outcome::{
    AmendConflict, AmendOutcome, ClaimConflict, ClaimOutcome, ClearConflict, ClearOutcome,
    InsertDependencyConflict, InsertDependencyOutcome, ReferenceConflict, ReferenceOutcome,
    RemoveAttachmentOutcome, ResolveConflict, ResolveOutcome, StaleRevision, TouchSessionConflict,
    TouchSessionOutcome,
};
pub use search::{
    SearchBackend, SearchError, SearchHit, SearchLimit, SearchOffset, SearchPage, SearchQuery,
    SearchRequest, SearchResult,
};
pub use session::{
    active_sessions, prepare_touch_session, session_of, session_state, SessionBudget, TouchInput,
};
pub use storage::{
    AllocatedId, AtomicWorkflows, AttachmentStore, CapacityLimit, Consistency, EntityReader,
    EntityWriter, IdAllocator, IdScope, InitiativeRevision, InitiativeScope, InitiativeSelector,
    Storage, StorageError, StorageResult,
};
pub use transition::{
    prepare_amend, prepare_claim, prepare_clear, prepare_dependency, prepare_resolution,
    AmendInput, ClaimInput, Decision as TransitionDecision, DependencyInput, ResolveInput,
};
