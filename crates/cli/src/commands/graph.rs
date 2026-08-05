//! `wayfind2 graph show`.

use std::str::FromStr;

use wayfind_core::derive::members_at;
use wayfind_core::error::ErrorToken;
use wayfind_core::id::SnapshotSelector;
use wayfind_core::render::{self, Field};

use super::Shell;
use crate::context::initiative_id;
use crate::error::ShellResult;
use crate::output::Output;

/// Show the graph at a snapshot, defaulting to the head.
pub fn show(shell: &Shell, snapshot: Option<&str>, output: &mut dyn Output) -> ShellResult<()> {
    let initiative = initiative_id(shell.chosen_initiative)?;
    let selector = match snapshot {
        Some(text) => SnapshotSelector::from_str(text)?,
        None => SnapshotSelector::Head,
    };

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

    let root = shell.graph.root_members(initiative)?;
    let transitions = shell.graph.accepted_transitions(initiative, ordinal)?;
    let members = members_at(&root, &transitions, ordinal);

    let mut nodes = Vec::with_capacity(members.nodes().len());
    for id in members.nodes() {
        let node = shell.graph.node(&id.hash())?.ok_or_else(|| {
            wayfind_core::storage::values::StorageError::CorruptData(format!(
                "node {id} is a member of snapshot {ordinal} but does not read back"
            ))
        })?;
        nodes.push(node);
    }

    let document =
        render::graph::graph_document(&members, initiative, ordinal, &nodes, &transitions);
    output.text(&document)
}
