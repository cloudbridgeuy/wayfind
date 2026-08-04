//! `wayfind2 initiative create` and the rest of the initiative group.
//!
//! Only `create` runs in this slice; every other member of the group is still
//! the A1 usage refusal.

use wayfind_core::command::graph::CreateInitiativeCommand;
use wayfind_core::error::ErrorToken;
use wayfind_core::outcome::graph::CreateInitiativeOutcome;
use wayfind_core::render::{self, Field};
use wayfind_core::storage::graph::GraphAppender;
use wayfind_core::validate::initiative::validate_create;

use super::Shell;
use crate::context::session_id;
use crate::error::ShellResult;
use crate::output::Output;
use crate::sqlite::graph_write::SqliteGraph;

/// Chart a new initiative and write its destination node as the first record
/// of an otherwise empty graph.
pub fn create(
    shell: &Shell,
    name: &str,
    destination: &str,
    notes: Option<&str>,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let created_by = session_id(shell.environment, shell.chosen_session.as_deref())?;
    let validated = validate_create(CreateInitiativeCommand {
        project: shell.project.clone(),
        name: name.to_string(),
        destination: destination.to_string(),
        notes: notes.map(str::to_string),
        created_by,
        now: shell.now,
    })?;
    let destination_node = validated.destination_node.id;
    let head = validated.snapshot.ordinal;

    let graph = SqliteGraph::new(shell.store.connection());
    match graph.create_initiative(validated)? {
        CreateInitiativeOutcome::Created(initiative) => {
            let document =
                render::initiative::initiative_document(&initiative, head, &destination_node);
            output.text(&document)
        }
        CreateInitiativeOutcome::NameTaken { existing } => {
            let rejection = wayfind_core::error::Rejection::new(ErrorToken::NameTaken)
                .key("initiative", Field::Number(existing.get()))
                .body("Another initiative in this project already holds that name.");
            Err(rejection.into())
        }
    }
}
