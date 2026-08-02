//! Wayfind's functional core.
//!
//! Every business rule lives here as a pure function over strict values. The
//! crate performs no input or output: it never reads a clock, a file, an
//! environment variable, or a database. The shell in `wayfind_cli` supplies
//! every effect as data and applies every decision this crate returns.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod command;
pub mod error;
pub mod id;
pub mod model;
pub mod outcome;
pub mod search;
pub mod storage;
pub mod time;

pub use command::{
    AddAttachmentReference, AddFogNote, AddScopeExclusion, AmendTicket, AttachmentName,
    ClaimTicket, ClearInitiative, CreateInitiative, CreateTicket, EnsureProject, InsertDependency,
    NonEmptyText, RemoveAttachmentReference, ResolutionText, ResolveTicket, StoreAttachment,
    TouchSession,
};
pub use error::{Error, Result};
pub use id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
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
pub use storage::{
    AllocatedId, AtomicWorkflows, AttachmentStore, CapacityLimit, Consistency, EntityReader,
    EntityWriter, IdAllocator, IdScope, InitiativeRevision, InitiativeScope, InitiativeSelector,
    Storage, StorageError, StorageResult,
};
