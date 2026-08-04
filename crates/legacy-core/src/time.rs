//! The one timestamp value the domain uses.
//!
//! Every stored timestamp comes from SQLite's `CURRENT_TIMESTAMP`, which writes
//! `YYYY-MM-DD HH:MM:SS` in UTC with no sub-second part. [`Timestamp`] parses
//! that form and prints it back unchanged, so a value written by the Rust
//! binary is indistinguishable from one written by the Bash script.
//!
//! The core never reads a clock. The shell reads `now` and passes it in.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};

/// The exact format SQLite's `CURRENT_TIMESTAMP` produces.
const SQL_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// The other shapes a stored or supplied timestamp may legitimately take.
const ACCEPTED_FORMATS: [&str; 3] = [
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%dT%H:%M:%S%.f",
];

/// A whole-second UTC instant, stored and printed in SQLite's timestamp format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(NaiveDateTime);

impl Timestamp {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "timestamp";

    /// The storage format, exposed so an adapter can bind the same string.
    pub const SQL_FORMAT: &'static str = SQL_FORMAT;

    /// Take a wall-clock reading from the shell, dropping any sub-second part.
    pub fn from_utc(now: DateTime<Utc>) -> Self {
        Self(truncate(now.naive_utc()))
    }

    /// Take an already-naive UTC reading, dropping any sub-second part.
    pub fn from_naive_utc(value: NaiveDateTime) -> Self {
        Self(truncate(value))
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
        self.0.format(SQL_FORMAT).to_string()
    }
}

/// Drop the sub-second part so equality and printing agree with storage.
fn truncate(value: NaiveDateTime) -> NaiveDateTime {
    value.with_nanosecond(0).unwrap_or(value)
}

impl FromStr for Timestamp {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        if let Ok(value) = NaiveDateTime::parse_from_str(text, SQL_FORMAT) {
            return Ok(Self(truncate(value)));
        }
        for format in ACCEPTED_FORMATS {
            if let Ok(value) = NaiveDateTime::parse_from_str(text, format) {
                return Ok(Self(truncate(value)));
            }
        }
        if let Ok(value) = DateTime::parse_from_rfc3339(text) {
            return Ok(Self::from_utc(value.into()));
        }
        Err(Error::invalid_value(
            Self::FIELD,
            format!("expected `{SQL_FORMAT}` in UTC, got {text:?}"),
        ))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(SQL_FORMAT))
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
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use chrono::{DateTime, Utc};

    use super::Timestamp;

    #[test]
    fn parses_the_sqlite_current_timestamp_format() {
        let value = Timestamp::from_str("2026-08-02 13:45:09").unwrap();
        assert_eq!(value.to_string(), "2026-08-02 13:45:09");
    }

    #[test]
    fn round_trips_through_storage_text() {
        let text = "2026-08-02 13:45:09";
        let value = Timestamp::from_str(text).unwrap();
        assert_eq!(value.to_storage_string(), text);
        assert_eq!(
            Timestamp::from_str(&value.to_storage_string()).unwrap(),
            value
        );
    }

    #[test]
    fn accepts_the_iso_and_fractional_variants_and_normalizes_them() {
        assert_eq!(
            Timestamp::from_str("2026-08-02T13:45:09")
                .unwrap()
                .to_string(),
            "2026-08-02 13:45:09"
        );
        assert_eq!(
            Timestamp::from_str("2026-08-02 13:45:09.250")
                .unwrap()
                .to_string(),
            "2026-08-02 13:45:09"
        );
        assert_eq!(
            Timestamp::from_str("2026-08-02T13:45:09Z")
                .unwrap()
                .to_string(),
            "2026-08-02 13:45:09"
        );
    }

    #[test]
    fn rejects_text_that_is_not_a_timestamp() {
        assert!(Timestamp::from_str("").is_err());
        assert!(Timestamp::from_str("not a date").is_err());
        assert!(Timestamp::from_str("2026-13-02 00:00:00").is_err());
        assert!(Timestamp::from_str("2026-08-02").is_err());
    }

    #[test]
    fn drops_the_sub_second_part_of_a_wall_clock_reading() {
        let now: DateTime<Utc> = DateTime::from_str("2026-08-02T13:45:09.987654321Z").unwrap();
        assert_eq!(Timestamp::from_utc(now).to_string(), "2026-08-02 13:45:09");
    }

    #[test]
    fn orders_chronologically() {
        let earlier = Timestamp::from_str("2026-08-02 13:45:09").unwrap();
        let later = Timestamp::from_str("2026-08-02 13:45:10").unwrap();
        assert!(earlier < later);
    }

    #[test]
    fn serializes_as_the_storage_string() {
        let value = Timestamp::from_str("2026-08-02 13:45:09").unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, "\"2026-08-02 13:45:09\"");
        assert_eq!(serde_json::from_str::<Timestamp>(&encoded).unwrap(), value);
        assert!(serde_json::from_str::<Timestamp>("\"nope\"").is_err());
    }
}
