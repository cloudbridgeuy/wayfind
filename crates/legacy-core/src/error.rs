//! The core's structured error type.
//!
//! Three cases, and they mean different things to a caller:
//!
//! - [`Error::InvalidValue`] — input never made it past the boundary. Blame the
//!   caller; report the field.
//! - [`Error::InvalidTransition`] — the input parsed, but the requested change
//!   is not legal from the current state. Blame the request, not the data.
//! - [`Error::CorruptData`] — a stored record holds a combination the domain
//!   says cannot exist. Blame the store; no retry will help.

use std::fmt;

/// Every failure the functional core can report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A value failed to parse at a boundary.
    InvalidValue {
        /// The name of the field that failed, as an operator would recognize it.
        field: &'static str,
        /// Why the value was rejected.
        reason: String,
    },
    /// A legal value asked for an illegal change of state.
    InvalidTransition {
        /// The entity the transition applied to, such as `"ticket"`.
        entity: &'static str,
        /// Why the transition was refused.
        reason: String,
    },
    /// A persisted record holds a combination the domain forbids.
    CorruptData {
        /// The entity whose stored form is impossible, such as `"ticket"`.
        entity: &'static str,
        /// What made the stored combination impossible.
        reason: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidValue { field, reason } => write!(f, "invalid {field}: {reason}"),
            Error::InvalidTransition { entity, reason } => {
                write!(f, "invalid {entity} transition: {reason}")
            }
            Error::CorruptData { entity, reason } => {
                write!(f, "corrupt {entity} record: {reason}")
            }
        }
    }
}

impl Error {
    /// Build an [`Error::InvalidValue`] without repeating the struct syntax.
    pub fn invalid_value(field: &'static str, reason: impl Into<String>) -> Self {
        Error::InvalidValue {
            field,
            reason: reason.into(),
        }
    }

    /// Build an [`Error::InvalidTransition`] without repeating the struct syntax.
    pub fn invalid_transition(entity: &'static str, reason: impl Into<String>) -> Self {
        Error::InvalidTransition {
            entity,
            reason: reason.into(),
        }
    }

    /// Build an [`Error::CorruptData`] without repeating the struct syntax.
    pub fn corrupt_data(entity: &'static str, reason: impl Into<String>) -> Self {
        Error::CorruptData {
            entity,
            reason: reason.into(),
        }
    }
}

/// The core's result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::Error;

    #[test]
    fn invalid_value_names_the_field_and_the_reason() {
        let error = Error::invalid_value("ticket id", "must be a positive integer, got 0");
        assert_eq!(
            error.to_string(),
            "invalid ticket id: must be a positive integer, got 0"
        );
    }

    #[test]
    fn invalid_transition_names_the_entity() {
        let error = Error::invalid_transition("ticket", "already claimed by another session");
        assert_eq!(
            error.to_string(),
            "invalid ticket transition: already claimed by another session"
        );
    }

    #[test]
    fn corrupt_data_names_the_entity() {
        let error = Error::corrupt_data("session", "closed session still holds a ticket");
        assert_eq!(
            error.to_string(),
            "corrupt session record: closed session still holds a ticket"
        );
    }

    #[test]
    fn the_three_cases_stay_distinct() {
        let invalid = Error::invalid_value("field", "reason");
        let transition = Error::invalid_transition("field", "reason");
        let corrupt = Error::corrupt_data("field", "reason");
        assert_ne!(invalid, transition);
        assert_ne!(transition, corrupt);
        assert_ne!(invalid, corrupt);
    }
}
