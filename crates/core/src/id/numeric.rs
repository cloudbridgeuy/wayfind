//! The positive-integer identifiers the coordination domain allocates.
//!
//! The immutable graph names its records by content hash. Everything in the
//! coordination domain — runs, questions, tickets — is allocated a rowid
//! instead, so those identifiers stay integers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// Refuse a value that cannot be this identifier.
///
/// Every numeric identifier refuses the same way, so the shape is written once:
/// the field is named as a key, the offending text as another, and the prose
/// says what was expected.
fn refuse(field: &'static str, value: impl Into<String>, expected: &str) -> Rejection {
    Rejection::new(ErrorToken::Usage)
        .key("field", Field::Text(field.to_string()))
        .key("value", Field::Text(value.into()))
        .body(expected.to_string())
}

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
            pub fn new(value: i64) -> Result<Self, Rejection> {
                if value < 1 {
                    return Err(crate::id::numeric::refuse(
                        $field,
                        value.to_string(),
                        "Expected a positive integer.",
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
            type Error = Rejection;

            fn try_from(value: i64) -> Result<Self, Rejection> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = Rejection;

            fn from_str(text: &str) -> Result<Self, Rejection> {
                let value: i64 = text.parse().map_err(|_| {
                    crate::id::numeric::refuse($field, text, "Expected a positive integer.")
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
                Self::new(value).map_err(|_| {
                    serde::de::Error::custom(format!(
                        "expected a positive integer for {}, got {value}",
                        $field
                    ))
                })
            }
        }
    };
}

// Named so a later module can instantiate a scope of its own. Every scope the
// current slice needs is declared below, in this file, so nothing imports it
// yet.
#[allow(unused_imports)]
pub(crate) use numeric_id;

numeric_id!(InitiativeId, "initiative id", "An initiative's identifier.");
numeric_id!(TicketId, "ticket id", "A ticket's identifier.");
numeric_id!(RunId, "run id", "A run's identifier.");
numeric_id!(QuestionId, "question id", "A question's identifier.");
numeric_id!(DecisionId, "decision id", "A decision's identifier.");
numeric_id!(
    NoteId,
    "note id",
    "A fog note's or scope exclusion's identifier. Both tables order by it."
);
numeric_id!(
    RunAttachmentId,
    "run attachment id",
    "A run attachment's identifier."
);

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{DecisionId, InitiativeId, NoteId, QuestionId, RunAttachmentId, RunId, TicketId};
    use crate::error::ErrorToken;

    #[test]
    fn numeric_ids_accept_positive_integers() {
        assert_eq!(TicketId::new(1).unwrap().get(), 1);
        assert_eq!(InitiativeId::new(42).unwrap().get(), 42);
        assert_eq!(RunId::new(7).unwrap().get(), 7);
        assert_eq!(QuestionId::new(4).unwrap().get(), 4);
        assert_eq!(DecisionId::new(9).unwrap().get(), 9);
        assert_eq!(NoteId::new(3).unwrap().get(), 3);
        assert_eq!(RunAttachmentId::new(11).unwrap().get(), 11);
    }

    #[test]
    fn numeric_ids_reject_zero_and_negative_values() {
        let rejection = TicketId::new(0).unwrap_err();
        assert_eq!(rejection.token(), ErrorToken::Usage);
        assert!(InitiativeId::new(-1).is_err());
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
    fn numeric_ids_sort_by_their_integer_value() {
        let mut ids = [
            TicketId::new(10).unwrap(),
            TicketId::new(2).unwrap(),
            TicketId::new(33).unwrap(),
        ];
        ids.sort();
        let sorted: Vec<i64> = ids.iter().map(|id| id.get()).collect();
        assert_eq!(sorted, vec![2, 10, 33]);
    }

    #[test]
    fn each_scope_names_itself_in_its_rejection() {
        assert_eq!(RunAttachmentId::FIELD, "run attachment id");
        let rejection = RunAttachmentId::new(0).unwrap_err();
        assert!(rejection.keys().iter().any(|(key, _)| *key == "field"));
    }
}
