//! The one timestamp value the domain uses.
//!
//! v2 writes and prints RFC 3339 in UTC with a `Z` suffix and no sub-second
//! part: `YYYY-MM-DDTHH:MM:SSZ`. v1 wrote SQLite's `YYYY-MM-DD HH:MM:SS`, which
//! is not RFC 3339 and which no off-the-shelf reader parses without being told
//! the format. The two stores are separate files, so the change costs nothing
//! and the v2 output is parseable everywhere.
//!
//! The core never reads a clock. The shell reads `now` and passes it in.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// The one format v2 reads back and prints.
const RFC3339_UTC: &str = "%Y-%m-%dT%H:%M:%SZ";

/// A whole-second UTC instant, stored and printed as RFC 3339 with `Z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(NaiveDateTime);

impl Timestamp {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "timestamp";

    /// The storage format, exposed so an adapter can bind the same string.
    pub const FORMAT: &'static str = RFC3339_UTC;

    /// Take a wall-clock reading from the shell, dropping any sub-second part.
    pub fn now_from(now: DateTime<Utc>) -> Self {
        Self(truncate(now.naive_utc()))
    }

    /// Read a stored or supplied RFC 3339 instant.
    ///
    /// Any offset is accepted and converted to UTC; a sub-second part is
    /// dropped. What comes back always prints in the one canonical form.
    pub fn parse_rfc3339(text: &str) -> Result<Self, Rejection> {
        DateTime::parse_from_rfc3339(text)
            .map(|value| Self::now_from(value.into()))
            .map_err(|_| {
                Rejection::new(ErrorToken::Usage)
                    .key("field", Field::Text(Self::FIELD.to_string()))
                    .key("value", Field::Text(text.to_string()))
                    .body("Expected an RFC 3339 instant, such as 2026-08-03T11:22:33Z.")
            })
    }

    /// The instant as a naive UTC date and time.
    pub fn naive_utc(self) -> NaiveDateTime {
        self.0
    }

    /// The instant as an aware UTC date and time.
    pub fn to_utc(self) -> DateTime<Utc> {
        self.0.and_utc()
    }

    /// The exact text a storage adapter should write.
    pub fn to_storage_string(self) -> String {
        self.0.format(RFC3339_UTC).to_string()
    }
}

/// Drop the sub-second part so equality and printing agree with storage.
fn truncate(value: NaiveDateTime) -> NaiveDateTime {
    value.with_nanosecond(0).unwrap_or(value)
}

impl FromStr for Timestamp {
    type Err = Rejection;

    fn from_str(text: &str) -> Result<Self, Rejection> {
        Self::parse_rfc3339(text)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(RFC3339_UTC))
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_storage_string())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Timestamp::parse_rfc3339(&text).map_err(|_| {
            serde::de::Error::custom(format!("expected an RFC 3339 instant, got {text:?}"))
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use chrono::{DateTime, Utc};

    use super::Timestamp;
    use crate::error::ErrorToken;

    #[test]
    fn renders_rfc3339_utc_z_form() {
        let t = Timestamp::parse_rfc3339("2026-08-03T11:22:33Z").unwrap();
        assert_eq!(t.to_string(), "2026-08-03T11:22:33Z");
    }

    #[test]
    fn an_offset_is_converted_to_utc_rather_than_kept() {
        let value = Timestamp::parse_rfc3339("2026-08-03T13:22:33+02:00").unwrap();
        assert_eq!(value.to_string(), "2026-08-03T11:22:33Z");
    }

    #[test]
    fn the_sub_second_part_is_dropped() {
        let value = Timestamp::parse_rfc3339("2026-08-03T11:22:33.987654321Z").unwrap();
        assert_eq!(value.to_string(), "2026-08-03T11:22:33Z");

        let now: DateTime<Utc> = DateTime::from_str("2026-08-03T11:22:33.987654321Z").unwrap();
        assert_eq!(Timestamp::now_from(now).to_string(), "2026-08-03T11:22:33Z");
    }

    #[test]
    fn round_trips_through_storage_text() {
        let text = "2026-08-03T11:22:33Z";
        let value = Timestamp::parse_rfc3339(text).unwrap();
        assert_eq!(value.to_storage_string(), text);
        assert_eq!(
            Timestamp::parse_rfc3339(&value.to_storage_string()).unwrap(),
            value
        );
    }

    #[test]
    fn text_that_is_not_an_instant_is_refused_with_the_field_named() {
        for text in ["", "not a date", "2026-13-02T00:00:00Z", "2026-08-03"] {
            let rejection = Timestamp::parse_rfc3339(text).unwrap_err();
            assert_eq!(rejection.token(), ErrorToken::Usage);
            assert!(rejection.keys().iter().any(|(key, _)| *key == "field"));
        }
    }

    #[test]
    fn the_v1_storage_format_is_no_longer_accepted() {
        assert!(Timestamp::parse_rfc3339("2026-08-03 11:22:33").is_err());
    }

    #[test]
    fn orders_chronologically() {
        let earlier = Timestamp::parse_rfc3339("2026-08-03T11:22:33Z").unwrap();
        let later = Timestamp::parse_rfc3339("2026-08-03T11:22:34Z").unwrap();
        assert!(earlier < later);
    }

    #[test]
    fn serializes_as_the_storage_string() {
        let value = Timestamp::parse_rfc3339("2026-08-03T11:22:33Z").unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, "\"2026-08-03T11:22:33Z\"");
        assert_eq!(serde_json::from_str::<Timestamp>(&encoded).unwrap(), value);
        assert!(serde_json::from_str::<Timestamp>("\"nope\"").is_err());
    }
}
