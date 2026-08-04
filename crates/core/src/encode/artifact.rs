//! Canonical encoding of an artifact's metadata.

use crate::encode::{header, write_field, CanonicalBytes};
use crate::id::RecordKind;
use crate::record::ArtifactDraft;

/// Encode an artifact in field order: description, byte length, content
/// hash, created_at, created_by.
pub fn encode_artifact(draft: &ArtifactDraft) -> CanonicalBytes {
    let mut buf = header(RecordKind::Artifact);
    write_field(&mut buf, "description", draft.description.as_bytes());
    write_field(
        &mut buf,
        "byte_size",
        draft.byte_size.to_string().as_bytes(),
    );
    write_field(
        &mut buf,
        "content_hash",
        draft.content_hash.to_hex().as_bytes(),
    );
    write_field(
        &mut buf,
        "created_at",
        draft.created_at.to_storage_string().as_bytes(),
    );
    write_field(&mut buf, "created_by", draft.created_by.as_str().as_bytes());
    CanonicalBytes::new(buf)
}
