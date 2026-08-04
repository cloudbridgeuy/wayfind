//! The content-address hash.
//!
//! Every record in the immutable graph is named by the SHA-256 of its
//! canonical encoding. This type is that hash, held as raw bytes so that
//! comparing two of them is a byte comparison and not a string one.

use std::fmt;

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// A SHA-256 content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Wrap 32 bytes already known to be a hash.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for a storage adapter to bind.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parse a lowercase, 64-character hex string.
    pub fn parse_hex(text: &str) -> Result<Self, Rejection> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(refuse(text));
        }

        let mut bytes = [0u8; 32];
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            // Every byte in `pair` already passed the hex-digit check above.
            let pair = std::str::from_utf8(pair).unwrap_or_default();
            bytes[index] = u8::from_str_radix(pair, 16).map_err(|_| refuse(text))?;
        }
        Ok(Self(bytes))
    }

    /// The lowercase, 64-character hex encoding.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// Refuse a string that is not 64 lowercase hex characters.
fn refuse(text: &str) -> Rejection {
    Rejection::new(ErrorToken::UnknownWord)
        .key("value", Field::Text(text.to_string()))
        .body("Expected 64 lowercase hex characters.")
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::Hash;
    use crate::error::ErrorToken;

    #[test]
    fn hex_round_trips_through_parse_and_render() {
        let hex = "a3".repeat(32);
        let hash = Hash::parse_hex(&hex).unwrap();
        assert_eq!(hash.to_hex(), hex);
    }

    #[test]
    fn parse_hex_rejects_the_wrong_length_or_case() {
        let short = "a".repeat(63);
        let long = "a".repeat(65);
        let upper = "A".repeat(64);
        for bad in [short, long, upper] {
            let rejection = Hash::parse_hex(&bad).unwrap_err();
            assert_eq!(rejection.token(), ErrorToken::UnknownWord);
        }
    }
}
