//! `wayfind2 snapshot list` and `snapshot show`.

use std::str::FromStr;

use wayfind_core::derive::members_at;
use wayfind_core::error::ErrorToken;
use wayfind_core::id::SnapshotSelector;
use wayfind_core::render::{self, Field};

use super::Shell;
use crate::context::initiative_id;
use crate::error::ShellResult;
use crate::output::Output;

/// List every snapshot of an initiative, in ordinal order.
pub fn list(shell: &Shell, output: &mut dyn Output) -> ShellResult<()> {
    let initiative = initiative_id(shell.chosen_initiative)?;
    let snapshots = shell.graph.snapshots(initiative)?;
    let document = render::graph::snapshot_list_document(initiative, &snapshots);
    output.text(&document)
}

/// Show one snapshot and its full membership.
pub fn show(shell: &Shell, selector: &str, output: &mut dyn Output) -> ShellResult<()> {
    let initiative = initiative_id(shell.chosen_initiative)?;
    let selector = SnapshotSelector::from_str(selector)?;

    let snapshots = shell.graph.snapshots(initiative)?;
    let ordinal = match selector {
        SnapshotSelector::Head => snapshots.iter().map(|snapshot| snapshot.ordinal).max(),
        SnapshotSelector::Ordinal(ordinal) => Some(ordinal).filter(|ordinal| {
            snapshots
                .iter()
                .any(|snapshot| snapshot.ordinal == *ordinal)
        }),
    };
    let Some(ordinal) = ordinal else {
        let rejection = wayfind_core::error::Rejection::new(ErrorToken::NotFound)
            .key("initiative", Field::Number(initiative.get()))
            .body("No snapshot holds that ordinal.");
        return Err(rejection.into());
    };

    let snapshot = shell.graph.snapshot(initiative, ordinal)?.ok_or_else(|| {
        wayfind_core::storage::values::StorageError::CorruptData(format!(
            "snapshot {ordinal} of initiative {} is listed but does not read back",
            initiative.get()
        ))
    })?;

    let root = shell.graph.root_members(initiative)?;
    let transitions = shell.graph.accepted_transitions(initiative, ordinal)?;
    let members = members_at(&root, &transitions, ordinal);

    let document = render::graph::snapshot_document(&snapshot, &members);
    output.text(&document)
}
