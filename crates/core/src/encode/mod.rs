//! The canonical byte encoding and content hash.
//!
//! Every record is named by the SHA-256 of its canonical encoding: a header
//! `wayfind <record-kind> <version>\0`, then each present field as
//! `<field-name>\0<byte-length>\0<bytes>`. An absent optional field is
//! skipped entirely rather than written empty — "absent" and "empty" must
//! hash differently, so the two cannot collapse into one representation.

pub mod artifact;
pub mod connection;
pub mod node;
pub mod transition;

use sha2::{Digest, Sha256};

use crate::id::{Hash, RecordId, RecordKind};

/// The canonical-encoding format version. Bumping this changes every hash.
pub const ENCODING_VERSION: u32 = 1;

/// The canonically encoded bytes of one draft, ready to hash.
pub struct CanonicalBytes(Vec<u8>);

impl CanonicalBytes {
    /// Wrap a byte sequence a submodule's encoder already built.
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The encoded bytes, for hashing or inspection.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Hash a byte sequence with SHA-256. Pure — no I/O.
pub fn hash_bytes(bytes: &[u8]) -> Hash {
    let digest = Sha256::digest(bytes);
    Hash::from_bytes(digest.into())
}

/// Name a record by hashing its canonical bytes.
pub fn identify(kind: RecordKind, bytes: &CanonicalBytes) -> RecordId {
    RecordId::new(kind, hash_bytes(bytes.as_slice()))
}

/// Start a record's byte sequence with its header.
pub(crate) fn header(kind: RecordKind) -> Vec<u8> {
    let mut buf = format!("wayfind {} {ENCODING_VERSION}", kind.as_encoding_word()).into_bytes();
    buf.push(0);
    buf
}

/// Append one present field: `<name>\0<byte-length>\0<bytes>`.
pub(crate) fn write_field(buf: &mut Vec<u8>, name: &str, value: &[u8]) {
    buf.extend_from_slice(name.as_bytes());
    buf.push(0);
    buf.extend_from_slice(value.len().to_string().as_bytes());
    buf.push(0);
    buf.extend_from_slice(value);
}

/// Append a field only when it is present; an absent one writes nothing.
pub(crate) fn write_field_opt(buf: &mut Vec<u8>, name: &str, value: Option<&[u8]>) {
    if let Some(value) = value {
        write_field(buf, name, value);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{hash_bytes, node::encode_node};
    use crate::id::SessionId;
    use crate::kinds::NodeKind;
    use crate::record::NodeDraft;
    use crate::time::Timestamp;

    fn draft() -> NodeDraft {
        NodeDraft {
            node_kind: NodeKind::Destination,
            title: "T".into(),
            summary: None,
            content: "C".into(),
            created_at: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
            created_by: SessionId::new("s").unwrap(),
        }
    }

    #[test]
    fn encoding_is_deterministic() {
        let a = encode_node(&draft());
        let b = encode_node(&draft());
        assert_eq!(a.as_slice(), b.as_slice());
    }

    #[test]
    fn a_different_created_at_changes_the_hash() {
        let mut other = draft();
        other.created_at = Timestamp::parse_rfc3339("2026-08-03T00:00:01Z").unwrap();
        assert_ne!(
            hash_bytes(encode_node(&draft()).as_slice()),
            hash_bytes(encode_node(&other).as_slice())
        );
    }

    #[test]
    fn a_different_created_by_changes_the_hash() {
        let mut other = draft();
        other.created_by = SessionId::new("other").unwrap();
        assert_ne!(
            hash_bytes(encode_node(&draft()).as_slice()),
            hash_bytes(encode_node(&other).as_slice())
        );
    }

    #[test]
    fn absent_and_empty_optional_differ() {
        let mut empty = draft();
        empty.summary = Some(String::new());
        assert_ne!(
            hash_bytes(encode_node(&draft()).as_slice()),
            hash_bytes(encode_node(&empty).as_slice())
        );
    }
}
