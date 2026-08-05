//! The identifiers of the immutable graph.

use std::fmt;
use std::str::FromStr;

use crate::error::{ErrorToken, Rejection};
use crate::id::Hash;
use crate::render::Field;

/// The four kinds of record the immutable graph stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordKind {
    Node,
    Transition,
    Connection,
    Artifact,
}

impl RecordKind {
    /// The letter this kind's identifiers are prefixed with.
    pub fn prefix(self) -> char {
        match self {
            Self::Node => 'R',
            Self::Transition => 'T',
            Self::Connection => 'C',
            Self::Artifact => 'A',
        }
    }

    /// The word this kind writes into a canonical encoding's header.
    pub fn as_encoding_word(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Transition => "transition",
            Self::Connection => "connection",
            Self::Artifact => "artifact",
        }
    }
}

/// The content-address identifier of one record in the immutable graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecordId {
    kind: RecordKind,
    hash: Hash,
}

impl RecordId {
    /// Pair a kind with the hash that names it.
    pub fn new(kind: RecordKind, hash: Hash) -> Self {
        Self { kind, hash }
    }

    /// The kind this identifier names a record of.
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// The hash this identifier is built from.
    pub fn hash(&self) -> Hash {
        self.hash
    }

    /// The short form an operator reads: the prefix, a dash, and eight hex
    /// characters.
    pub fn abbreviated(&self) -> String {
        format!("{}-{}", self.kind.prefix(), &self.hash.to_hex()[..8])
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.kind.prefix(), self.hash.to_hex())
    }
}

impl FromStr for RecordId {
    type Err = Rejection;

    /// Parse the full form only: a prefix, a dash, and the full 64-character
    /// hash. A prefix goes through `HexPrefix` instead — mixing the two forms
    /// here would let an ambiguous prefix silently become an address.
    fn from_str(text: &str) -> Result<Self, Rejection> {
        let (prefix, hex) = text.split_once('-').ok_or_else(|| refuse(text))?;
        let kind = match prefix {
            "R" => RecordKind::Node,
            "T" => RecordKind::Transition,
            "C" => RecordKind::Connection,
            "A" => RecordKind::Artifact,
            _ => return Err(refuse(text)),
        };
        let hash = Hash::parse_hex(hex).map_err(|_| refuse(text))?;
        Ok(Self { kind, hash })
    }
}

/// Refuse a string that is not a full record identifier.
fn refuse(text: &str) -> Rejection {
    Rejection::new(ErrorToken::UnknownWord)
        .key("value", Field::Text(text.to_string()))
        .body("Expected a full record identifier, such as R-<64 hex characters>.")
}

/// The position of one snapshot in an initiative's history, counting from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotOrdinal(u32);

impl SnapshotOrdinal {
    /// Build an ordinal, refusing zero.
    pub fn new(value: u32) -> Result<Self, Rejection> {
        if value < 1 {
            return Err(Rejection::new(ErrorToken::UnknownWord)
                .key("value", Field::Text(value.to_string()))
                .body("Expected a snapshot ordinal of 1 or more."));
        }
        Ok(Self(value))
    }

    /// The underlying integer, for a storage adapter to bind.
    pub fn get(self) -> u32 {
        self.0
    }

    /// The ordinal that follows this one.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for SnapshotOrdinal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S{}", self.0)
    }
}

impl FromStr for SnapshotOrdinal {
    type Err = Rejection;

    /// Parse either `"S4"` or the bare `"4"`.
    fn from_str(text: &str) -> Result<Self, Rejection> {
        let refuse = || {
            Rejection::new(ErrorToken::UnknownWord)
                .key("value", Field::Text(text.to_string()))
                .body("Expected a snapshot ordinal, such as S4 or 4.")
        };
        let digits = text.strip_prefix('S').unwrap_or(text);
        let value: u32 = digits.parse().map_err(|_| refuse())?;
        Self::new(value).map_err(|_| refuse())
    }
}

/// Which snapshot a read command means: the head, or a specific ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotSelector {
    /// The snapshot with the highest ordinal.
    Head,
    /// One specific snapshot.
    Ordinal(SnapshotOrdinal),
}

impl FromStr for SnapshotSelector {
    type Err = Rejection;

    /// Parse `"head"`, `"S4"`, or the bare `"4"`.
    fn from_str(text: &str) -> Result<Self, Rejection> {
        if text == "head" {
            return Ok(Self::Head);
        }
        SnapshotOrdinal::from_str(text).map(Self::Ordinal)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::RecordId;

    #[test]
    fn record_id_renders_full_and_abbreviated() {
        let id = RecordId::from_str(
            "R-0000000000000000000000000000000000000000000000000000000000000abc",
        )
        .unwrap();
        assert_eq!(id.to_string().len(), 66);
        assert_eq!(id.abbreviated(), "R-00000000");
    }

    #[test]
    fn record_id_from_str_refuses_a_prefix() {
        assert!(RecordId::from_str("R-a3f9").is_err());
    }

    #[test]
    fn snapshot_ordinal_parses_the_prefixed_and_bare_forms_alike() {
        use super::SnapshotOrdinal;

        let prefixed = SnapshotOrdinal::from_str("S4").unwrap();
        let bare = SnapshotOrdinal::from_str("4").unwrap();
        assert_eq!(prefixed, bare);
        assert_eq!(prefixed.to_string(), "S4");
    }

    #[test]
    fn snapshot_ordinal_rejects_zero() {
        use super::SnapshotOrdinal;

        assert!(SnapshotOrdinal::from_str("S0").is_err());
        assert!(SnapshotOrdinal::from_str("0").is_err());
    }

    #[test]
    fn snapshot_selector_parses_head_and_both_ordinal_forms() {
        use super::{SnapshotOrdinal, SnapshotSelector};

        assert_eq!(
            SnapshotSelector::from_str("head").unwrap(),
            SnapshotSelector::Head
        );
        assert_eq!(
            SnapshotSelector::from_str("S4").unwrap(),
            SnapshotSelector::Ordinal(SnapshotOrdinal::new(4).unwrap())
        );
        assert_eq!(
            SnapshotSelector::from_str("4").unwrap(),
            SnapshotSelector::Ordinal(SnapshotOrdinal::new(4).unwrap())
        );
    }
}
