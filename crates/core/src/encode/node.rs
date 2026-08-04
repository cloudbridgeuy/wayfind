//! Canonical encoding of a result node.

use crate::encode::{header, write_field, write_field_opt, CanonicalBytes};
use crate::id::RecordKind;
use crate::record::NodeDraft;

/// Encode a result node in field order: node kind, title, summary, content,
/// created_at, created_by.
pub fn encode_node(draft: &NodeDraft) -> CanonicalBytes {
    let mut buf = header(RecordKind::Node);
    write_field(&mut buf, "node_kind", draft.node_kind.as_token().as_bytes());
    write_field(&mut buf, "title", draft.title.as_bytes());
    write_field_opt(
        &mut buf,
        "summary",
        draft.summary.as_deref().map(str::as_bytes),
    );
    write_field(&mut buf, "content", draft.content.as_bytes());
    write_field(
        &mut buf,
        "created_at",
        draft.created_at.to_storage_string().as_bytes(),
    );
    write_field(&mut buf, "created_by", draft.created_by.as_str().as_bytes());
    CanonicalBytes::new(buf)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::encode_node;
    use crate::id::SessionId;
    use crate::kinds::NodeKind;
    use crate::record::NodeDraft;
    use crate::time::Timestamp;

    #[test]
    fn header_and_fields_follow_the_git_object_shape() {
        let draft = NodeDraft {
            node_kind: NodeKind::Destination,
            title: "T".into(),
            summary: None,
            content: "C".into(),
            created_at: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
            created_by: SessionId::new("s").unwrap(),
        };
        let bytes = encode_node(&draft);
        let s = String::from_utf8_lossy(bytes.as_slice());
        assert!(s.starts_with("wayfind node 1\0"));
        assert!(s.contains("title\0"));
        assert!(
            !s.contains("summary"),
            "an absent optional field is skipped entirely"
        );
    }
}
