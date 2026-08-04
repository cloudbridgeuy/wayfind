//! The closed enums a stored row spells out as one word.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};

/// The four kinds of ticket, matching the `tickets.type` check constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketType {
    /// A question to grill an assumption with.
    Grilling,
    /// An investigation that gathers facts without settling design.
    Research,
    /// A build-to-learn spike.
    Prototype,
    /// Ordinary work with a definite finish.
    Task,
}

impl TicketType {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "ticket type";

    /// Every variant, in the order the check constraint lists them.
    pub const ALL: [TicketType; 4] = [
        TicketType::Grilling,
        TicketType::Research,
        TicketType::Prototype,
        TicketType::Task,
    ];

    /// The exact text this variant is stored as.
    pub fn as_str(self) -> &'static str {
        match self {
            TicketType::Grilling => "grilling",
            TicketType::Research => "research",
            TicketType::Prototype => "prototype",
            TicketType::Task => "task",
        }
    }

    /// Whether resolving this ticket leaves a session free to take another.
    ///
    /// A session may resolve any number of research tickets, but only one
    /// non-research ticket. This is the predicate that rule reads.
    pub fn is_research(self) -> bool {
        matches!(self, TicketType::Research)
    }
}

impl FromStr for TicketType {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "grilling" => Ok(TicketType::Grilling),
            "research" => Ok(TicketType::Research),
            "prototype" => Ok(TicketType::Prototype),
            "task" => Ok(TicketType::Task),
            other => Err(Error::invalid_value(
                Self::FIELD,
                format!("expected one of grilling, research, prototype, task; got {other:?}"),
            )),
        }
    }
}

impl fmt::Display for TicketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TicketType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The status column of an initiative, exactly as the database stores it.
///
/// This is not the same as [`InitiativeState`]. The stored status records a
/// deliberate operator action; the state is computed from the status *and* the
/// ticket graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistedInitiativeStatus {
    /// Created, no work committed yet.
    Charting,
    /// Work is under way.
    Working,
    /// Deliberately closed by `initiative clear`.
    Clear,
}

impl PersistedInitiativeStatus {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "initiative status";

    /// The exact text this variant is stored as.
    pub fn as_str(self) -> &'static str {
        match self {
            PersistedInitiativeStatus::Charting => "charting",
            PersistedInitiativeStatus::Working => "working",
            PersistedInitiativeStatus::Clear => "clear",
        }
    }

    /// Whether an implicit write should refuse to touch this initiative.
    pub fn is_clear(self) -> bool {
        matches!(self, PersistedInitiativeStatus::Clear)
    }
}

impl FromStr for PersistedInitiativeStatus {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "charting" => Ok(PersistedInitiativeStatus::Charting),
            "working" => Ok(PersistedInitiativeStatus::Working),
            "clear" => Ok(PersistedInitiativeStatus::Clear),
            other => Err(Error::invalid_value(
                Self::FIELD,
                format!("expected one of charting, working, clear; got {other:?}"),
            )),
        }
    }
}

impl fmt::Display for PersistedInitiativeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PersistedInitiativeStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}
