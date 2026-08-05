//! An identifier prefix: enough hex to narrow a search, not enough to be an
//! address on its own.
//!
//! [`crate::id::RecordId::from_str`] deliberately refuses a prefix — mixing
//! the two forms there would let an ambiguous prefix silently become an
//! address. This is the other form: a kind and a short run of lowercase hex,
//! resolved against a store's candidates by [`resolve`].

use std::fmt;
use std::str::FromStr;

use crate::error::{ErrorToken, Rejection};
use crate::id::record::{RecordId, RecordKind};
use crate::outcome::graph::ResolveOutcome;
use crate::render::Field;

/// The shortest prefix a caller may type: fewer hex characters than this
/// could name most of a large store at once.
const MIN_HEX: usize = 4;

/// The longest prefix a caller may type: a full hash, spelled as a prefix.
const MAX_HEX: usize = 64;

/// A record kind and a run of lowercase hex, such as `R-a3f9`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexPrefix {
    kind: RecordKind,
    hex: String,
}

impl HexPrefix {
    /// The kind this prefix searches within.
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// The hex characters typed, lowercase.
    pub fn hex(&self) -> &str {
        &self.hex
    }
}

impl fmt::Display for HexPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.kind.prefix(), self.hex)
    }
}

impl FromStr for HexPrefix {
    type Err = Rejection;

    /// Parse `"R-a3f9"`: a kind letter, a dash, and 4 to 64 lowercase hex
    /// characters.
    fn from_str(text: &str) -> Result<Self, Rejection> {
        let (letter, hex) = text.split_once('-').ok_or_else(|| refuse(text))?;
        let kind = match letter {
            "R" => RecordKind::Node,
            "T" => RecordKind::Transition,
            "C" => RecordKind::Connection,
            "A" => RecordKind::Artifact,
            _ => return Err(refuse(text)),
        };
        let well_formed = (MIN_HEX..=MAX_HEX).contains(&hex.len())
            && hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if !well_formed {
            return Err(refuse(text));
        }
        Ok(Self {
            kind,
            hex: hex.to_string(),
        })
    }
}

/// Resolve a prefix against a store's candidates.
///
/// A candidate of a different [`RecordKind`] never matches, even when its hex
/// agrees — the kind letter narrows the search before the hex does.
pub fn resolve(prefix: &HexPrefix, candidates: &[RecordId]) -> ResolveOutcome {
    let mut matches: Vec<RecordId> = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.kind() == prefix.kind && candidate.hash().to_hex().starts_with(&prefix.hex)
        })
        .collect();
    matches.sort();

    match matches.len() {
        0 => ResolveOutcome::Unknown,
        1 => ResolveOutcome::Unique(matches[0]),
        _ => ResolveOutcome::Ambiguous(matches),
    }
}

/// Refuse a string that is not a well-formed prefix.
///
/// This is `ErrorToken::UnknownWord` on the `id` field: a short or malformed
/// prefix is not a missing record, it is a word that is not spelled right.
fn refuse(text: &str) -> Rejection {
    Rejection::new(ErrorToken::UnknownWord)
        .key("id", Field::Text(text.to_string()))
        .body("Expected a record identifier or prefix, such as R-a3f9.")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{resolve, HexPrefix};
    use crate::id::{Hash, RecordId, RecordKind};
    use crate::outcome::graph::ResolveOutcome;

    #[test]
    fn prefix_needs_four_hex_characters() {
        assert!(HexPrefix::from_str("R-a3f").is_err());
        assert!(HexPrefix::from_str("R-a3f9").is_ok());
        assert!(
            HexPrefix::from_str("R-A3F9").is_err(),
            "uppercase is not the canonical form"
        );
        assert!(
            HexPrefix::from_str("X-a3f9").is_err(),
            "unknown kind prefix"
        );
    }

    fn node(hex_prefix: &str) -> RecordId {
        let hex = format!("{hex_prefix}{}", "0".repeat(64 - hex_prefix.len()));
        RecordId::new(RecordKind::Node, Hash::parse_hex(&hex).unwrap())
    }

    #[test]
    fn no_candidate_matches_is_unknown() {
        let prefix = HexPrefix::from_str("R-a3f9").unwrap();
        let candidates = vec![node("b7000000")];
        assert_eq!(resolve(&prefix, &candidates), ResolveOutcome::Unknown);
    }

    #[test]
    fn one_candidate_matches_is_unique() {
        let prefix = HexPrefix::from_str("R-a3f9").unwrap();
        let only = node("a3f90000");
        let candidates = vec![node("b7000000"), only];
        assert_eq!(resolve(&prefix, &candidates), ResolveOutcome::Unique(only));
    }

    #[test]
    fn two_candidates_match_is_ambiguous_sorted_by_hex() {
        let prefix = HexPrefix::from_str("R-a3f9").unwrap();
        let higher = node("a3f9ffff");
        let lower = node("a3f90000");
        let candidates = vec![higher, lower];
        assert_eq!(
            resolve(&prefix, &candidates),
            ResolveOutcome::Ambiguous(vec![lower, higher])
        );
    }

    #[test]
    fn a_different_kind_never_matches_even_when_the_hex_agrees() {
        let prefix = HexPrefix::from_str("R-a3f9").unwrap();
        let transition = RecordId::new(
            RecordKind::Transition,
            Hash::parse_hex(&format!("a3f9{}", "0".repeat(60))).unwrap(),
        );
        assert_eq!(resolve(&prefix, &[transition]), ResolveOutcome::Unknown);
    }
}
