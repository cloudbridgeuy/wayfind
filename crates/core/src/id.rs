//! Strict identifier values.
//!
//! Every identifier is a private-field newtype that can only be built by
//! parsing. Once a caller holds one, the value is known good: a numeric id is
//! positive, a project key is an absolute physical path, and a session id is a
//! short line of printable text.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};

/// Define a positive-integer identifier newtype.
///
/// SQLite hands back `INTEGER PRIMARY KEY` rowids as `i64`. Wayfind only ever
/// allocates from 1 upward, so zero and negative values are rejected instead of
/// being carried around as "probably fine".
macro_rules! numeric_id {
    ($name:ident, $field:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(i64);

        impl $name {
            /// The operator-facing name of this identifier, used in errors.
            pub const FIELD: &'static str = $field;

            /// Parse a raw integer into this identifier.
            pub fn new(value: i64) -> Result<Self> {
                if value < 1 {
                    return Err(Error::invalid_value(
                        $field,
                        format!("must be a positive integer, got {value}"),
                    ));
                }
                Ok(Self(value))
            }

            /// The underlying integer, for a storage adapter to bind.
            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl TryFrom<i64> for $name {
            type Error = Error;

            fn try_from(value: i64) -> Result<Self> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(text: &str) -> Result<Self> {
                let value: i64 = text.parse().map_err(|_| {
                    Error::invalid_value(
                        $field,
                        format!("expected a positive integer, got {text:?}"),
                    )
                })?;
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                let value = i64::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

numeric_id!(InitiativeId, "initiative id", "An initiative's identifier.");
numeric_id!(TicketId, "ticket id", "A ticket's identifier.");
numeric_id!(AttachmentId, "attachment id", "An attachment's identifier.");
numeric_id!(DecisionId, "decision id", "A decision's identifier.");
numeric_id!(
    NoteId,
    "note id",
    "A fog note's or scope exclusion's identifier. Both tables order by it."
);

/// The longest session identifier Wayfind accepts.
const SESSION_ID_MAX_CHARS: usize = 256;

/// A project's key: the absolute physical path of its git root or working
/// directory.
///
/// The Bash script derives this with `pwd -P`, so it is always absolute and
/// already has symbolic links resolved. Parsing keeps that guarantee explicit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ProjectKey(String);

impl ProjectKey {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "project key";

    /// Parse a raw path string into a project key.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::invalid_value(Self::FIELD, "must not be empty"));
        }
        if !value.starts_with('/') {
            return Err(Error::invalid_value(
                Self::FIELD,
                format!("must be an absolute physical path, got {value:?}"),
            ));
        }
        if value.contains('\0') {
            return Err(Error::invalid_value(
                Self::FIELD,
                "must not hold a NUL byte",
            ));
        }
        Ok(Self(value))
    }

    /// The key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the key and return the owned path string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl FromStr for ProjectKey {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl fmt::Display for ProjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProjectKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A session's identifier.
///
/// Sessions are named by the agent runtime, so the value is opaque text rather
/// than a number. It still has to survive being printed inside a TOML string
/// and inside one cell of a Markdown table, so control characters are rejected.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "session id";

    /// Parse raw text into a session identifier.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::invalid_value(Self::FIELD, "must not be empty"));
        }
        if value.chars().count() > SESSION_ID_MAX_CHARS {
            return Err(Error::invalid_value(
                Self::FIELD,
                format!("must be at most {SESSION_ID_MAX_CHARS} characters"),
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

    /// The identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the identifier and return the owned text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl FromStr for SessionId {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{AttachmentId, DecisionId, InitiativeId, NoteId, ProjectKey, SessionId, TicketId};
    use crate::error::Error;

    #[test]
    fn numeric_ids_accept_positive_integers() {
        assert_eq!(TicketId::new(1).unwrap().get(), 1);
        assert_eq!(InitiativeId::new(42).unwrap().get(), 42);
        assert_eq!(AttachmentId::new(7).unwrap().get(), 7);
        assert_eq!(DecisionId::new(9).unwrap().get(), 9);
        assert_eq!(NoteId::new(3).unwrap().get(), 3);
    }

    #[test]
    fn numeric_ids_reject_zero_and_negative_values() {
        assert!(matches!(
            TicketId::new(0),
            Err(Error::InvalidValue {
                field: "ticket id",
                ..
            })
        ));
        assert!(matches!(
            InitiativeId::new(-1),
            Err(Error::InvalidValue {
                field: "initiative id",
                ..
            })
        ));
    }

    #[test]
    fn numeric_ids_parse_from_text_without_trimming() {
        assert_eq!(TicketId::from_str("12").unwrap().get(), 12);
        assert!(TicketId::from_str(" 12").is_err());
        assert!(TicketId::from_str("12 ").is_err());
        assert!(TicketId::from_str("12x").is_err());
        assert!(TicketId::from_str("").is_err());
        assert!(TicketId::from_str("0").is_err());
    }

    #[test]
    fn numeric_ids_display_as_bare_integers() {
        assert_eq!(TicketId::new(12).unwrap().to_string(), "12");
    }

    #[test]
    fn numeric_ids_round_trip_through_json() {
        let id = TicketId::new(12).unwrap();
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(encoded, "12");
        assert_eq!(serde_json::from_str::<TicketId>(&encoded).unwrap(), id);
        assert!(serde_json::from_str::<TicketId>("0").is_err());
    }

    #[test]
    fn project_keys_require_an_absolute_path() {
        assert_eq!(
            ProjectKey::new("/Users/example/project").unwrap().as_str(),
            "/Users/example/project"
        );
        assert!(ProjectKey::new("").is_err());
        assert!(ProjectKey::new("relative/path").is_err());
        assert!(ProjectKey::new("/holds\0nul").is_err());
    }

    #[test]
    fn session_ids_accept_ordinary_identifiers() {
        let id = SessionId::new("0199a3f0-1c4d-7a20-9f0b-8a4c1f2d3e4f").unwrap();
        assert_eq!(id.as_str(), "0199a3f0-1c4d-7a20-9f0b-8a4c1f2d3e4f");
        assert_eq!(id.to_string(), "0199a3f0-1c4d-7a20-9f0b-8a4c1f2d3e4f");
    }

    #[test]
    fn session_ids_reject_empty_control_and_overlong_text() {
        assert!(SessionId::new("").is_err());
        assert!(SessionId::new("has\nnewline").is_err());
        assert!(SessionId::new("has\ttab").is_err());
        assert!(SessionId::new("has\0nul").is_err());
        assert!(SessionId::new("a".repeat(256)).is_ok());
        assert!(SessionId::new("a".repeat(257)).is_err());
    }

    #[test]
    fn session_ids_round_trip_through_json() {
        let id = SessionId::new("session-1").unwrap();
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(encoded, "\"session-1\"");
        assert_eq!(serde_json::from_str::<SessionId>(&encoded).unwrap(), id);
        assert!(serde_json::from_str::<SessionId>("\"\"").is_err());
    }

    #[test]
    fn numeric_ids_sort_by_their_integer_value() {
        let mut ids = vec![
            TicketId::new(10).unwrap(),
            TicketId::new(2).unwrap(),
            TicketId::new(33).unwrap(),
        ];
        ids.sort();
        let sorted: Vec<i64> = ids.iter().map(|id| id.get()).collect();
        assert_eq!(sorted, vec![2, 10, 33]);
    }
}
