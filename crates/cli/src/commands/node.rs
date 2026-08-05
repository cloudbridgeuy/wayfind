//! `wayfind2 node show`.

use std::str::FromStr;

use wayfind_core::error::ErrorToken;
use wayfind_core::id::prefix::{resolve, HexPrefix};
use wayfind_core::id::RecordKind;
use wayfind_core::outcome::graph::ResolveOutcome;
use wayfind_core::render::{self, Field};

use super::Shell;
use crate::error::ShellResult;
use crate::output::Output;

/// Show one result node, addressed by a full id or an unambiguous prefix.
pub fn show(shell: &Shell, id: &str, output: &mut dyn Output) -> ShellResult<()> {
    let prefix = HexPrefix::from_str(id)?;
    let candidates = shell.graph.resolve_prefix(RecordKind::Node, prefix.hex())?;

    let node_id = match resolve(&prefix, &candidates) {
        ResolveOutcome::Unique(id) => id,
        ResolveOutcome::Ambiguous(candidates) => {
            let rejection = wayfind_core::error::Rejection::new(ErrorToken::AmbiguousId)
                .key("prefix", Field::Text(prefix.to_string()))
                .key(
                    "candidates",
                    Field::Ids(candidates.iter().map(ToString::to_string).collect()),
                )
                .body("More than one node matches that prefix.");
            return Err(rejection.into());
        }
        ResolveOutcome::Unknown => {
            let rejection = wayfind_core::error::Rejection::new(ErrorToken::NotFound)
                .key("id", Field::Text(prefix.to_string()))
                .body("No node holds that id.");
            return Err(rejection.into());
        }
    };

    let node = shell.graph.node(&node_id.hash())?.ok_or_else(|| {
        wayfind_core::storage::values::StorageError::CorruptData(format!(
            "node {node_id} resolved from a prefix but does not read back"
        ))
    })?;

    let document = render::graph::node_document(&node);
    output.text(&document)
}
