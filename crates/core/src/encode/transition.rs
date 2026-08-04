//! Canonical encoding of a transition.

use crate::encode::{header, write_field, write_field_opt, CanonicalBytes};
use crate::id::{RecordId, RecordKind};
use crate::record::TransitionDraft;

/// Encode a transition in field order: transition kind, summary, rationale?,
/// input hashes (manifest order), output hashes (manifest order),
/// created_at, created_by; an import transition then adds source initiative,
/// source snapshot ordinal, included hashes (sorted), import rationale.
pub fn encode_transition(draft: &TransitionDraft) -> CanonicalBytes {
    let mut buf = header(RecordKind::Transition);
    write_field(
        &mut buf,
        "transition_kind",
        draft.transition_kind.as_token().as_bytes(),
    );
    write_field(&mut buf, "summary", draft.summary.as_bytes());
    write_field_opt(
        &mut buf,
        "rationale",
        draft.rationale.as_deref().map(str::as_bytes),
    );
    for input in &draft.inputs {
        write_field(&mut buf, "input", input.hash().to_hex().as_bytes());
    }
    for output in &draft.outputs {
        write_field(&mut buf, "output", output.hash().to_hex().as_bytes());
    }
    write_field(
        &mut buf,
        "created_at",
        draft.created_at.to_storage_string().as_bytes(),
    );
    write_field(&mut buf, "created_by", draft.created_by.as_str().as_bytes());
    if let Some(import) = &draft.import {
        write_field(
            &mut buf,
            "source_initiative",
            import.source_initiative.to_string().as_bytes(),
        );
        write_field(
            &mut buf,
            "source_snapshot",
            import.source_snapshot.get().to_string().as_bytes(),
        );
        let mut included: Vec<RecordId> = import.included.clone();
        included.sort();
        for record in &included {
            write_field(&mut buf, "included", record.hash().to_hex().as_bytes());
        }
        write_field(&mut buf, "import_rationale", import.rationale.as_bytes());
    }
    CanonicalBytes::new(buf)
}
