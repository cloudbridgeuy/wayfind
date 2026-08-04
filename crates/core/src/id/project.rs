//! The two identifiers that are text rather than numbers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// The longest session identifier Wayfind accepts.
const SESSION_ID_MAX_CHARS: usize = 256;

/// Refuse text that cannot be one of these identifiers.
fn refuse(field: &'static str, value: &str, expected: impl Into<String>) -> Rejection {
    Rejection::new(ErrorToken::Usage)
        .key("field", Field::Text(field.to_string()))
        .key("value", Field::Text(value.to_string()))
        .body(expected)
}

/// A project's key: the absolute physical path of its git root or working
/// directory.
///
/// The shell derives this with the physical path, so it is always absolute and
/// already has symbolic links resolved. Parsing keeps that guarantee explicit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ProjectKey(String);

impl ProjectKey {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "project key";

    /// Parse a raw path string into a project key.
    pub fn new(value: impl Into<String>) -> Result<Self, Rejection> {
        let value = value.into();
        if value.is_empty() {
            return Err(refuse(Self::FIELD, &value, "Expected a non-empty path."));
        }
        if !value.starts_with('/') {
            return Err(refuse(
                Self::FIELD,
                &value,
                "Expected an absolute physical path.",
            ));
        }
        if value.contains('\0') {
            return Err(refuse(
                Self::FIELD,
                &value,
                "Expected a path with no NUL byte.",
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
    type Err = Rejection;

    fn from_str(text: &str) -> Result<Self, Rejection> {
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
        Self::new(&value).map_err(|_| {
            serde::de::Error::custom(format!("expected an absolute physical path, got {value:?}"))
        })
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
    pub fn new(value: impl Into<String>) -> Result<Self, Rejection> {
        let value = value.into();
        if value.is_empty() {
            return Err(refuse(Self::FIELD, &value, "Expected non-empty text."));
        }
        if value.chars().count() > SESSION_ID_MAX_CHARS {
            return Err(refuse(
                Self::FIELD,
                &value,
                format!("Expected at most {SESSION_ID_MAX_CHARS} characters."),
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(refuse(
                Self::FIELD,
                &value,
                "Expected text with no control character.",
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
    type Err = Rejection;

    fn from_str(text: &str) -> Result<Self, Rejection> {
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
        Self::new(&value)
            .map_err(|_| serde::de::Error::custom(format!("not a session id: {value:?}")))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{ProjectKey, SessionId};

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
    fn both_round_trip_through_json() {
        let key = ProjectKey::new("/Users/example/project").unwrap();
        let encoded = serde_json::to_string(&key).unwrap();
        assert_eq!(encoded, "\"/Users/example/project\"");
        assert_eq!(serde_json::from_str::<ProjectKey>(&encoded).unwrap(), key);
        assert!(serde_json::from_str::<ProjectKey>("\"relative\"").is_err());

        let id = SessionId::new("session-1").unwrap();
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(encoded, "\"session-1\"");
        assert_eq!(serde_json::from_str::<SessionId>(&encoded).unwrap(), id);
        assert!(serde_json::from_str::<SessionId>("\"\"").is_err());
    }
}
