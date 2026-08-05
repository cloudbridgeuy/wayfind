//! `wayfind2 initiative create`, `list`, and `show`.
//!
//! `close`, `clone`, and `import` are still the A1 usage refusal.

use wayfind_core::command::graph::CreateInitiativeCommand;
use wayfind_core::error::ErrorToken;
use wayfind_core::id::{InitiativeId, RecordKind};
use wayfind_core::outcome::graph::CreateInitiativeOutcome;
use wayfind_core::render::{self, Field};
use wayfind_core::storage::values::StorageError;
use wayfind_core::validate::initiative::validate_create;

use super::Shell;
use crate::context::session_id;
use crate::error::ShellResult;
use crate::output::Output;

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

    match shell.graph.create_initiative(validated)? {
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

/// List every initiative of the current project.
pub fn list(shell: &Shell, output: &mut dyn Output) -> ShellResult<()> {
    let initiatives = shell.graph.initiatives(&shell.project)?;
    let document = render::initiative::initiative_list_document(&shell.project, &initiatives);
    output.text(&document)
}

/// Show one initiative at its head snapshot.
pub fn show(shell: &Shell, id: i64, output: &mut dyn Output) -> ShellResult<()> {
    let initiative_id = InitiativeId::new(id)?;
    let initiative = shell.graph.initiative(initiative_id)?.ok_or_else(|| {
        wayfind_core::error::Rejection::new(ErrorToken::NotFound)
            .key("initiative", Field::Number(id))
            .body("No initiative holds that id.")
    })?;

    let snapshots = shell.graph.snapshots(initiative_id)?;
    let head = snapshots
        .iter()
        .map(|snapshot| snapshot.ordinal)
        .max()
        .ok_or_else(|| StorageError::CorruptData(format!("initiative {id} has no snapshots")))?;

    let root_members = shell.graph.root_members(initiative_id)?;
    let destination = root_members
        .iter()
        .find(|member| member.kind() == RecordKind::Node)
        .copied()
        .ok_or_else(|| {
            StorageError::CorruptData(format!(
                "initiative {id} has no destination node among its root members"
            ))
        })?;

    let document = render::initiative::initiative_document(&initiative, head, &destination);
    output.text(&document)
}
