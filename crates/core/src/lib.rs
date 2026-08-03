//! Wayfind's functional core.
//!
//! Every business rule lives here as a pure function over strict values. The
//! crate performs no input or output: it never reads a clock, a file, an
//! environment variable, or a database. The shell in `wayfind_cli` supplies
//! every effect as data and applies every decision this crate returns.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod command;
#[cfg(test)]
mod compat_tests;
pub mod config;
pub mod error;
pub mod format;
pub mod graph;
pub mod id;
pub mod initiative;
pub mod model;
pub mod outcome;
pub mod render;
pub mod search;
pub mod session;
pub mod storage;
pub mod time;
pub mod transition;
pub mod tree;

pub use command::{
    AddAttachmentReference, AddFogNote, AddScopeExclusion, AmendTicket, AttachmentName,
    ClaimTicket, ClearInitiative, CreateInitiative, CreateTicket, EnsureProject, InsertDependency,
    NonEmptyText, RemoveAttachmentReference, ResolutionText, ResolveTicket, StoreAttachment,
    TouchSession,
};
pub use config::{
    resolve_config, ConfigInput, ConfigSource, RawConfig, RawReserved, RawSqlite, RawSqliteFts5,
    ResolvedConfig, SearchConfig, SearchSelector, SqliteFts5Settings, SqliteSettings,
    StorageConfig, StorageSelector, AVAILABLE_SEARCH_BACKENDS, AVAILABLE_STORAGE_BACKENDS,
    DEFAULT_SEARCH_BACKEND, DEFAULT_SEARCH_TABLE, DEFAULT_STORAGE_BACKEND, RESERVED_BACKENDS,
};
pub use error::{Error, Result};
pub use format::{
    clamp_gist, flatten_lines, format_size, strip_one_trailing_newline, toml_string, ELLIPSIS,
    GIST_LIMIT,
};
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
pub use render::{
    render_attachment_header, render_attachment_list, render_csv, render_handoff, render_init,
    render_initiative_cleared, render_map, render_next_unavailable, render_search,
    render_session_list, render_session_resume, render_ticket, state_guidance, AttachmentListView,
    AttachmentRow, AttachmentView, DecisionRow, DumpRow, Field, FrontMatter, FrontierRow,
    FullDecision, HandoffView, InitiativeHeader, MapView, NextView, OwnedAttachmentRow,
    ReferencedAttachmentRow, SearchView, SessionListView, SessionResumeView, SessionRow,
    TicketView, UnresolvedRow, DUMP_HEADER,
};
pub use search::{
    SearchBackend, SearchError, SearchHit, SearchLimit, SearchOffset, SearchPage, SearchQuery,
    SearchRequest, SearchResult, FTS5_METADATA_NAMESPACE,
};
pub use session::{
    active_sessions, prepare_touch_session, session_of, session_state, SessionBudget, TouchInput,
};
pub use storage::{
    AllocatedId, AtomicWorkflows, AttachmentStore, CapacityLimit, Consistency, EntityReader,
    EntityWriter, IdAllocator, IdScope, InitiativeRevision, InitiativeScope, InitiativeSelector,
    Storage, StorageError, StorageResult, MAX_BYTES_PER_WORKFLOW, MAX_ITEMS_PER_WORKFLOW,
};
pub use time::Timestamp;
pub use transition::{
    prepare_amend, prepare_claim, prepare_clear, prepare_dependency, prepare_resolution,
    AmendInput, ClaimInput, Decision as TransitionDecision, DependencyInput, ResolveInput,
};
pub use tree::{render_tree, TreeRow, TreeView};
