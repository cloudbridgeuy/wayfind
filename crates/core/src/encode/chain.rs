//! The snapshot chain hash.
//!
//! A chain hash lets a snapshot verify replay without becoming an address:
//! the root's is a hash over its sorted membership, and every later
//! snapshot's folds in the previous chain hash and the transition that
//! produced it.

use crate::encode::hash_bytes;
use crate::id::{Hash, RecordId};

/// The root snapshot's chain hash: a hash over the sorted member hashes.
pub fn root_chain_hash(members: &[RecordId]) -> Hash {
    let mut hashes: Vec<Hash> = members.iter().map(RecordId::hash).collect();
    hashes.sort();
    let mut buf = Vec::new();
    for hash in hashes {
        buf.extend_from_slice(hash.as_bytes());
    }
    hash_bytes(&buf)
}

/// A non-root snapshot's chain hash: a hash over (previous chain hash,
/// transition hash).
pub fn next_chain_hash(previous: &Hash, transition: &RecordId) -> Hash {
    let mut buf = Vec::new();
    buf.extend_from_slice(previous.as_bytes());
    buf.extend_from_slice(transition.hash().as_bytes());
    hash_bytes(&buf)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::root_chain_hash;
    use crate::id::{Hash, RecordId, RecordKind};

    #[test]
    fn root_chain_hash_does_not_depend_on_input_order() {
        let a = RecordId::new(RecordKind::Node, Hash::parse_hex(&"11".repeat(32)).unwrap());
        let b = RecordId::new(RecordKind::Node, Hash::parse_hex(&"22".repeat(32)).unwrap());
        assert_eq!(root_chain_hash(&[a, b]), root_chain_hash(&[b, a]));
    }
}
