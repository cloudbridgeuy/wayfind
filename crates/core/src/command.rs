//! The write requests the shell hands to a store.
//!
//! Every command is a complete description of one write: which records it
//! touches, what state it expects to find them in, which identifiers it has
//! already reserved, and what the clock read when the shell decided. Nothing in
//! a command has to be looked up again, and nothing in it is a suggestion — a
//! backend either finds every expectation met and applies the whole thing, or
//! reports the first expectation that failed as a conflict value.
//!
//! Two consequences are worth stating plainly.
//!
//! Commands carry identifiers rather than asking for them. A command applied
//! twice writes the same rows twice at the same identifiers, so a retry after a
//! timeout cannot silently create a second ticket.
//!
//! Commands carry `now` rather than reading a clock. The core has no clock, and
//! a replayed command therefore writes the timestamp of the decision instead of
//! the timestamp of the replay.
//!
//! A command must stay inside the limits in [`crate::storage`]:
//! [`MAX_ITEMS_PER_WORKFLOW`](crate::storage::MAX_ITEMS_PER_WORKFLOW) records
//! and [`MAX_BYTES_PER_WORKFLOW`](crate::storage::MAX_BYTES_PER_WORKFLOW)
//! bytes. SQLite would accept more; the limits exist so a command written today
//! stays applicable by a backend that cannot.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};
use crate::id::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
use crate::model::TicketType;
use crate::storage::InitiativeRevision;
use crate::time::Timestamp;

// ---------------------------------------------------------------------------
// Command input values
// ---------------------------------------------------------------------------

/// Text that the domain requires to carry something.
///
/// The Bash script rejects an empty initiative name, an empty ticket title, an
/// empty note, and an empty resolution. This is that rule as a type, so the
/// check happens once at the edge instead of at every use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct NonEmptyText(String);

impl NonEmptyText {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "text";

    /// Parse raw text, rejecting text that is empty or only whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        Self::named(Self::FIELD, value)
    }

    /// Parse raw text, naming the field so the error says which one was empty.
    pub fn named(field: &'static str, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::invalid_value(field, "must not be empty"));
        }
        Ok(Self(value))
    }

    /// The text as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the value and return the owned text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl FromStr for NonEmptyText {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl fmt::Display for NonEmptyText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NonEmptyText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// The text of a decision.
///
/// A resolution is the one piece of free text the domain reads back rather than
/// only printing: its first line becomes the decision gist that the map and the
/// handoff list. Keeping the split in one place stops two callers from
/// disagreeing about what "the first line" means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ResolutionText(String);

impl ResolutionText {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "resolution";

    /// Parse raw text into a resolution.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::invalid_value(Self::FIELD, "must not be empty"));
        }
        Ok(Self(value))
    }

    /// The full recorded text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first line, which is what the store keeps as the decision gist.
    ///
    /// This matches the Bash script's `gist="${resolution%%$'\n'*}"` exactly:
    /// the split is on the first newline, with no trimming and no shortening.
    /// Shortening a gist for display is a formatting concern, not a stored one.
    pub fn gist(&self) -> &str {
        match self.0.split_once('\n') {
            Some((first, _)) => first,
            None => &self.0,
        }
    }

    /// Consume the value and return the owned text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl FromStr for ResolutionText {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl fmt::Display for ResolutionText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ResolutionText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// An attachment's file name.
///
/// The name is the label an operator types back at Wayfind, and it is unique
/// within its ticket. It is never used to open a path, but it is printed inside
/// a TOML string and inside one cell of a Markdown table, so a name that holds a
/// separator, a control character, or a directory traversal is rejected at the
/// edge rather than escaped at every use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct AttachmentName(String);

impl AttachmentName {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "attachment name";

    /// The longest attachment name Wayfind accepts.
    pub const MAX_CHARS: usize = 255;

    /// Parse raw text into an attachment name.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::invalid_value(Self::FIELD, "must not be empty"));
        }
        if value.chars().count() > Self::MAX_CHARS {
            return Err(Error::invalid_value(
                Self::FIELD,
                format!("must be at most {} characters", Self::MAX_CHARS),
            ));
        }
        if value == "." || value == ".." {
            return Err(Error::invalid_value(
                Self::FIELD,
                format!("must name a document, got {value:?}"),
            ));
        }
        if value.contains('/') || value.contains('\\') {
            return Err(Error::invalid_value(
                Self::FIELD,
                "must not hold a path separator",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(Error::invalid_value(
                Self::FIELD,
                "must not hold a control character",
            ));
        }
        Ok(Self(value))
    }

    /// The name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the value and return the owned text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl FromStr for AttachmentName {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl fmt::Display for AttachmentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AttachmentName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Entity commands
// ---------------------------------------------------------------------------

/// Record a project if it is not recorded yet.
///
/// Idempotent by design: the first `wayfind` command run in a directory creates
/// the project, and every later one finds it. No revision is expected because
/// there is nothing to race with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureProject {
    /// The project to record.
    pub key: ProjectKey,
    /// The clock reading to write if the project is new.
    pub now: Timestamp,
}

/// Create an initiative at an already-reserved identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateInitiative {
    /// The identifier reserved for it.
    pub id: InitiativeId,
    /// The project it belongs to.
    pub project_key: ProjectKey,
    /// Its name, unique within the project.
    pub name: NonEmptyText,
    /// Where the work is going.
    pub destination: NonEmptyText,
    /// Free-form framing, which may be empty.
    pub notes: String,
    /// The clock reading to write.
    pub now: Timestamp,
}

/// Create a ticket at an already-reserved identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTicket {
    /// The identifier reserved for it.
    pub id: TicketId,
    /// The initiative it joins.
    pub initiative_id: InitiativeId,
    /// Its title, unique within the initiative.
    pub title: NonEmptyText,
    /// Its kind.
    pub ticket_type: TicketType,
    /// The question it asks, which may be empty.
    pub question: String,
    /// The clock reading to write.
    pub now: Timestamp,
}

/// Repair the recorded text of a resolved ticket.
///
/// Amending rewrites a transcription fault. It never changes a decision, never
/// reopens a ticket, and never re-claims one. It touches two records — the
/// ticket and its decision — so it names the revision it decided against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendTicket {
    /// The ticket to repair.
    pub ticket_id: TicketId,
    /// The initiative the ticket must still belong to.
    pub initiative_id: InitiativeId,
    /// The revision the shell read the ticket at.
    pub expected_initiative_revision: InitiativeRevision,
    /// The corrected text.
    pub resolution: ResolutionText,
    /// The clock reading to write as `amended_at`.
    pub now: Timestamp,
}

/// Close an initiative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearInitiative {
    /// The initiative to close.
    pub initiative_id: InitiativeId,
    /// The revision the shell confirmed "no open or claimed tickets" at.
    pub expected_initiative_revision: InitiativeRevision,
    /// The clock reading, kept for backends that record a closing time.
    pub now: Timestamp,
}

/// Record that a session is alive, creating it if this is its first command.
///
/// Every command starts with one of these. It writes only `last_seen_at`, which
/// no stable read reports, so it deliberately carries no expected revision and
/// deliberately does not move one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TouchSession {
    /// The session to record.
    pub session_id: SessionId,
    /// The project it works in.
    pub project_key: ProjectKey,
    /// The initiative it is bound to, if the project has one.
    pub initiative_id: Option<InitiativeId>,
    /// The clock reading to write as `last_seen_at`.
    pub now: Timestamp,
}

/// Add a "not yet specified" note at an already-reserved identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFogNote {
    /// The identifier reserved for it.
    pub id: NoteId,
    /// The initiative it belongs to.
    pub initiative_id: InitiativeId,
    /// The note text.
    pub note: NonEmptyText,
    /// The clock reading to write.
    pub now: Timestamp,
}

/// Add an "out of scope" note at an already-reserved identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddScopeExclusion {
    /// The identifier reserved for it.
    pub id: NoteId,
    /// The initiative it belongs to.
    pub initiative_id: InitiativeId,
    /// The exclusion text.
    pub note: NonEmptyText,
    /// The clock reading to write.
    pub now: Timestamp,
}

// ---------------------------------------------------------------------------
// Atomic workflow commands
// ---------------------------------------------------------------------------

/// Take a ticket for a session.
///
/// Four records move together: the ticket's status, the claim row, the session's
/// held ticket, and the initiative's revision. The command names what each one
/// must look like first, so a backend can refuse without a second read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimTicket {
    /// The ticket to take.
    pub ticket_id: TicketId,
    /// The initiative the ticket must belong to.
    pub initiative_id: InitiativeId,
    /// The revision the shell decided against.
    pub expected_initiative_revision: InitiativeRevision,
    /// The session taking it.
    pub session_id: SessionId,
    /// The ticket the session already holds, which must be nothing or this one.
    pub expected_session_holds: Option<TicketId>,
    /// The session that already holds the ticket, which must be nothing or this
    /// session. A re-claim by the same session is allowed and is not a conflict.
    pub expected_claimant: Option<SessionId>,
    /// The clock reading to write as `claimed_at` and `last_seen_at`.
    pub now: Timestamp,
}

/// Record a decision and release the session.
///
/// Five records move together: the ticket, the claim row, the session's held
/// ticket and non-research counter, the new decision, and the initiative's
/// revision. The decision's identifier is reserved before the command is built,
/// so a retry writes the same decision rather than a duplicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveTicket {
    /// The ticket to settle.
    pub ticket_id: TicketId,
    /// The initiative the ticket must belong to.
    pub initiative_id: InitiativeId,
    /// The revision the shell decided against.
    pub expected_initiative_revision: InitiativeRevision,
    /// The session settling it, which must currently hold it.
    pub session_id: SessionId,
    /// The identifier reserved for the new decision.
    pub decision_id: DecisionId,
    /// The kind of the ticket, which decides whether the session spends its one
    /// non-research resolution.
    pub ticket_type: TicketType,
    /// The decision text.
    pub resolution: ResolutionText,
    /// The clock reading to write as `resolved_at`, `released_at`, and
    /// `last_seen_at`.
    pub now: Timestamp,
}

impl ResolveTicket {
    /// Whether this resolution spends the session's non-research budget.
    ///
    /// A research ticket is free; anything else costs the session's one slot.
    pub fn spends_non_research_budget(&self) -> bool {
        !self.ticket_type.is_research()
    }
}

/// Add one dependency edge.
///
/// Both tickets must exist in the same initiative, and the edge must not close a
/// cycle. Whether it would is decided in the core against the graph the shell
/// read at `expected_initiative_revision`; the store's job is to confirm nothing
/// moved since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsertDependency {
    /// The ticket that waits.
    pub ticket_id: TicketId,
    /// The ticket that must resolve first.
    pub blocker_id: TicketId,
    /// The initiative both tickets must belong to.
    pub initiative_id: InitiativeId,
    /// The revision the cycle check was run against.
    pub expected_initiative_revision: InitiativeRevision,
    /// The clock reading, kept for backends that record an edge's creation time.
    pub now: Timestamp,
}

// ---------------------------------------------------------------------------
// Attachment commands
// ---------------------------------------------------------------------------

/// Store one document at an already-reserved identifier.
///
/// The bytes travel beside the command rather than inside it, so a caller can
/// stream them and so a command stays cheap to clone, compare, and log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreAttachment {
    /// The identifier reserved for it.
    pub id: AttachmentId,
    /// The ticket that will own it.
    pub ticket_id: TicketId,
    /// Its file name, unique within its ticket.
    pub name: AttachmentName,
    /// What it holds.
    pub description: String,
    /// The session storing it, if one is known.
    pub session_id: Option<SessionId>,
    /// The clock reading to write.
    pub now: Timestamp,
}

/// Point a ticket at an attachment another ticket owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddAttachmentReference {
    /// The attachment being pointed at.
    pub attachment_id: AttachmentId,
    /// The ticket doing the pointing.
    pub ticket_id: TicketId,
    /// The clock reading to write.
    pub now: Timestamp,
}

/// Drop a pointer from a ticket to an attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveAttachmentReference {
    /// The attachment that was pointed at.
    pub attachment_id: AttachmentId,
    /// The ticket that was pointing.
    pub ticket_id: TicketId,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{AttachmentName, NonEmptyText, ResolutionText, ResolveTicket};
    use crate::error::Error;
    use crate::id::{DecisionId, InitiativeId, SessionId, TicketId};
    use crate::model::TicketType;
    use crate::storage::InitiativeRevision;
    use crate::time::Timestamp;

    fn resolve(ticket_type: TicketType) -> ResolveTicket {
        ResolveTicket {
            ticket_id: TicketId::new(1).unwrap(),
            initiative_id: InitiativeId::new(1).unwrap(),
            expected_initiative_revision: InitiativeRevision::INITIAL,
            session_id: SessionId::new("session-1").unwrap(),
            decision_id: DecisionId::new(1).unwrap(),
            ticket_type,
            resolution: ResolutionText::new("settled").unwrap(),
            now: Timestamp::from_str("2026-08-02 13:45:09").unwrap(),
        }
    }

    #[test]
    fn non_empty_text_rejects_blank_input_and_names_the_field() {
        assert_eq!(NonEmptyText::new("a name").unwrap().as_str(), "a name");
        assert!(NonEmptyText::new("").is_err());
        assert!(NonEmptyText::new("   \t\n ").is_err());
        assert!(matches!(
            NonEmptyText::named("initiative name", ""),
            Err(Error::InvalidValue {
                field: "initiative name",
                ..
            })
        ));
    }

    #[test]
    fn non_empty_text_keeps_the_surrounding_whitespace_it_was_given() {
        assert_eq!(
            NonEmptyText::new("  padded  ").unwrap().as_str(),
            "  padded  "
        );
    }

    #[test]
    fn a_resolution_gist_is_the_first_line_untouched() {
        assert_eq!(ResolutionText::new("one line").unwrap().gist(), "one line");
        assert_eq!(
            ResolutionText::new("first line\nsecond line\nthird")
                .unwrap()
                .gist(),
            "first line"
        );
        assert_eq!(
            ResolutionText::new("  padded first  \nrest")
                .unwrap()
                .gist(),
            "  padded first  "
        );
        assert_eq!(ResolutionText::new("\nbody").unwrap().gist(), "");
    }

    #[test]
    fn a_resolution_must_carry_text() {
        assert!(ResolutionText::new("").is_err());
        assert!(ResolutionText::new("\n\n  ").is_err());
    }

    #[test]
    fn attachment_names_reject_paths_and_control_characters() {
        assert_eq!(
            AttachmentName::new("notes.md").unwrap().as_str(),
            "notes.md"
        );
        assert!(AttachmentName::new("").is_err());
        assert!(AttachmentName::new(".").is_err());
        assert!(AttachmentName::new("..").is_err());
        assert!(AttachmentName::new("../escape.md").is_err());
        assert!(AttachmentName::new("dir/notes.md").is_err());
        assert!(AttachmentName::new("dir\\notes.md").is_err());
        assert!(AttachmentName::new("two\nlines.md").is_err());
        assert!(AttachmentName::new("a".repeat(255)).is_ok());
        assert!(AttachmentName::new("a".repeat(256)).is_err());
    }

    #[test]
    fn only_a_non_research_resolution_spends_the_session_budget() {
        assert!(!resolve(TicketType::Research).spends_non_research_budget());
        assert!(resolve(TicketType::Grilling).spends_non_research_budget());
        assert!(resolve(TicketType::Prototype).spends_non_research_budget());
        assert!(resolve(TicketType::Task).spends_non_research_budget());
    }

    #[test]
    fn commands_round_trip_through_json() {
        let command = resolve(TicketType::Research);
        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(
            serde_json::from_str::<ResolveTicket>(&encoded).unwrap(),
            command
        );
    }

    #[test]
    fn command_text_values_round_trip_through_json() {
        let name = AttachmentName::new("notes.md").unwrap();
        assert_eq!(serde_json::to_string(&name).unwrap(), "\"notes.md\"");
        assert!(serde_json::from_str::<AttachmentName>("\"a/b\"").is_err());
        assert!(serde_json::from_str::<NonEmptyText>("\"\"").is_err());
        assert!(serde_json::from_str::<ResolutionText>("\" \"").is_err());
    }
}
