//! The storage boundary's shared values, carried over from v1.
//!
//! **Conflicts are values, not errors.** A duplicate name or a stale write is
//! an ordinary outcome the caller must handle, reported through an outcome
//! type, never through [`StorageError`]. [`StorageError`] is reserved for the
//! three things a caller cannot plan around: the store being unreachable, the
//! store holding impossible data, and the store refusing a request that
//! exceeds a capacity limit.
//!
//! **Identifiers are allocated, never discovered.** No write returns an
//! identifier as a side effect of inserting.

use std::fmt;

use crate::id::{InitiativeId, NoteId, QuestionId, RunAttachmentId, RunId, TicketId};

/// A limit the store refused to exceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapacityLimit {
    /// One workflow named more participants than a backend can commit at once.
    ItemsPerWorkflow {
        /// The ceiling.
        limit: usize,
        /// How many the command asked for.
        requested: usize,
    },
    /// One workflow carried more bytes than a backend can commit at once.
    BytesPerWorkflow {
        /// The ceiling.
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

impl fmt::Display for CapacityLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    /// The store could not be reached, or refused the request for a reason
    /// the domain has no opinion about.
    #[error("storage failed during {operation}: {detail}")]
    Infrastructure {
        /// The operation that failed, for the operator's benefit.
        operation: &'static str,
        /// What the backend reported.
        detail: String,
    },
    /// The store returned a record the domain says cannot exist.
    #[error("the store holds data the domain says cannot exist: {0}")]
    CorruptData(String),
    /// The request exceeded a declared capacity limit.
    #[error("storage capacity limit: {0}")]
    CapacityExceeded(CapacityLimit),
}

impl StorageError {
    /// Build a [`StorageError::Infrastructure`] without repeating the syntax.
    pub fn infrastructure(operation: &'static str, detail: impl Into<String>) -> Self {
        StorageError::Infrastructure {
            operation,
            detail: detail.into(),
        }
    }
}

/// The result type every storage capability returns.
pub type StorageResult<T> = std::result::Result<T, StorageError>;

/// How fresh a read has to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Consistency {
    /// The read must observe every write that committed before it started.
    Strong,
    /// The read may lag. Acceptable for display-only listings.
    Relaxed,
}

/// A counter that changes whenever a run's guarded stable view changes.
///
/// v1 guarded the initiative's stable view; v2 moves the guard to the run,
/// since the immutable graph carries no revision of its own.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RunRevision(u64);

impl RunRevision {
    /// The revision a run starts at.
    pub const INITIAL: RunRevision = RunRevision(0);

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
    /// every optimistic write, which is safe. A revision that wraps would
    /// start accepting stale ones.
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for RunRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which counter to draw the next identifier from.
///
/// No graph scope exists here: the immutable graph names its records by
/// content hash, so it needs no allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdScope {
    /// `tickets.id`
    Ticket,
    /// `runs.id`
    Run,
    /// `questions.id`
    Question,
    /// `run_attachments.id`
    Attachment,
    /// `notes.id`
    Note,
    /// `initiatives.id`
    Initiative,
}

impl IdScope {
    /// The scope's name, used in errors.
    pub fn as_str(self) -> &'static str {
        match self {
            IdScope::Ticket => "ticket",
            IdScope::Run => "run",
            IdScope::Question => "question",
            IdScope::Attachment => "attachment",
            IdScope::Note => "note",
            IdScope::Initiative => "initiative",
        }
    }
}

/// A freshly allocated identifier, tagged with the scope it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocatedId {
    /// A ticket identifier.
    Ticket(TicketId),
    /// A run identifier.
    Run(RunId),
    /// A question identifier.
    Question(QuestionId),
    /// A run attachment identifier.
    Attachment(RunAttachmentId),
    /// A note identifier.
    Note(NoteId),
    /// An initiative identifier.
    Initiative(InitiativeId),
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
    allocated_id_accessor!(ticket, Ticket, TicketId, "ticket");
    allocated_id_accessor!(run, Run, RunId, "run");
    allocated_id_accessor!(question, Question, QuestionId, "question");
    allocated_id_accessor!(attachment, Attachment, RunAttachmentId, "attachment");
    allocated_id_accessor!(note, Note, NoteId, "note");
    allocated_id_accessor!(initiative, Initiative, InitiativeId, "initiative");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{AllocatedId, CapacityLimit, Consistency, IdScope, RunRevision, StorageError};
    use crate::id::TicketId;

    #[test]
    fn revisions_start_at_zero_and_move_forward() {
        let first = RunRevision::INITIAL;
        assert_eq!(first.get(), 0);
        assert_eq!(first.next().get(), 1);
        assert!(first < first.next());
    }

    #[test]
    fn revisions_saturate_rather_than_wrap() {
        let last = RunRevision::new(u64::MAX);
        assert_eq!(last.next(), last);
    }

    #[test]
    fn allocated_ids_unwrap_only_their_own_scope() {
        let allocated = AllocatedId::Ticket(TicketId::new(7).unwrap());
        assert_eq!(allocated.ticket().unwrap(), TicketId::new(7).unwrap());
        assert!(allocated.run().is_err());
    }

    #[test]
    fn id_scopes_name_themselves() {
        assert_eq!(IdScope::Run.as_str(), "run");
        assert_eq!(IdScope::Ticket.as_str(), "ticket");
    }

    #[test]
    fn consistency_levels_are_distinct() {
        assert_ne!(Consistency::Strong, Consistency::Relaxed);
    }

    #[test]
    fn capacity_limits_explain_themselves() {
        let limit = CapacityLimit::ItemsPerWorkflow {
            limit: 100,
            requested: 101,
        };
        assert_eq!(
            StorageError::CapacityExceeded(limit).to_string(),
            "storage capacity limit: a single workflow may touch at most 100 items, but this one names 101"
        );
    }

    #[test]
    fn infrastructure_errors_name_the_operation() {
        let error = StorageError::infrastructure("create_initiative", "database is locked");
        assert_eq!(
            error.to_string(),
            "storage failed during create_initiative: database is locked"
        );
    }
}
