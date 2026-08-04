//! Canonical encoding of a connection.

use crate::encode::{header, write_field, CanonicalBytes};
use crate::id::RecordKind;
use crate::record::ConnectionDraft;

/// Encode a connection in field order: owning transition hash, from hash, to
/// hash, relation, created_at.
pub fn encode_connection(draft: &ConnectionDraft) -> CanonicalBytes {
    let mut buf = header(RecordKind::Connection);
    write_field(
        &mut buf,
        "transition",
        draft.transition.hash().to_hex().as_bytes(),
    );
    write_field(&mut buf, "from", draft.from.hash().to_hex().as_bytes());
    write_field(&mut buf, "to", draft.to.hash().to_hex().as_bytes());
    write_field(&mut buf, "relation", draft.relation.as_token().as_bytes());
    write_field(
        &mut buf,
        "created_at",
        draft.created_at.to_storage_string().as_bytes(),
    );
    CanonicalBytes::new(buf)
}
