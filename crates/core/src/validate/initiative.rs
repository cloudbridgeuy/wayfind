//! Validating `CreateInitiativeCommand` into a fully hashed write.

use crate::command::graph::CreateInitiativeCommand;
use crate::encode::chain::root_chain_hash;
use crate::encode::node::encode_node;
use crate::encode::{identify, CanonicalBytes};
use crate::error::Rejection;
use crate::id::{Hash, ProjectKey, RecordId, RecordKind, SnapshotOrdinal};
use crate::kinds::NodeKind;
use crate::record::NodeDraft;
use crate::time::Timestamp;

/// A draft plus the identity and bytes its encoding produced — ready for the
/// shell to write without recomputing anything.
pub struct Prepared<D> {
    pub id: RecordId,
    pub bytes: CanonicalBytes,
    pub draft: D,
}

/// The root snapshot a new initiative starts at.
pub struct PreparedRootSnapshot {
    pub ordinal: SnapshotOrdinal,
    pub chain_hash: Hash,
    pub created_at: Timestamp,
}

/// A `CreateInitiativeCommand` that has been checked and fully hashed.
///
/// The `_sealed` field is private, so this struct is constructible only from
/// within this module — an unvalidated write must be unspellable.
pub struct ValidatedInitiative {
    pub project: ProjectKey,
    pub name: String,
    pub destination: String,
    pub notes: Option<String>,
    pub destination_node: Prepared<NodeDraft>,
    pub snapshot: PreparedRootSnapshot,
    _sealed: (),
}

/// Check and fully hash a `CreateInitiativeCommand`.
///
/// The destination node's encoding is fixed by S3 Detail M3, so a migrated
/// initiative and a v2-born one hash alike: `title` is the initiative name,
/// `summary` is the destination text, and `content` is the destination text
/// plus, when notes are non-empty, one blank line and then the notes.
pub fn validate_create(cmd: CreateInitiativeCommand) -> Result<ValidatedInitiative, Rejection> {
    let content = match cmd.notes.as_deref() {
        Some(notes) if !notes.is_empty() => format!("{}\n\n{notes}", cmd.destination),
        _ => cmd.destination.clone(),
    };
    let draft = NodeDraft {
        node_kind: NodeKind::Destination,
        title: cmd.name.clone(),
        summary: Some(cmd.destination.clone()),
        content,
        created_at: cmd.now,
        created_by: cmd.created_by,
    };
    let bytes = encode_node(&draft);
    let id = identify(RecordKind::Node, &bytes);
    let chain_hash = root_chain_hash(&[id]);

    Ok(ValidatedInitiative {
        project: cmd.project,
        name: cmd.name,
        destination: cmd.destination,
        notes: cmd.notes,
        destination_node: Prepared { id, bytes, draft },
        snapshot: PreparedRootSnapshot {
            ordinal: SnapshotOrdinal::new(1)?,
            chain_hash,
            created_at: cmd.now,
        },
        _sealed: (),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::validate_create;
    use crate::command::graph::CreateInitiativeCommand;
    use crate::id::{ProjectKey, SessionId};
    use crate::time::Timestamp;

    fn command(notes: Option<&str>) -> CreateInitiativeCommand {
        CreateInitiativeCommand {
            project: ProjectKey::new("/repo").unwrap(),
            name: "Ship v2".into(),
            destination: "d".into(),
            notes: notes.map(String::from),
            created_by: SessionId::new("s").unwrap(),
            now: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
        }
    }

    #[test]
    fn destination_content_appends_notes_after_one_blank_line() {
        let validated = validate_create(command(Some("n"))).unwrap();
        assert_eq!(validated.destination_node.draft.content, "d\n\nn");

        let validated = validate_create(command(None)).unwrap();
        assert_eq!(validated.destination_node.draft.content, "d");
    }
}
