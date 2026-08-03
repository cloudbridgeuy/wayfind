//! `wayfind map`: the whole initiative on one page.

use std::fmt::Write as _;

use super::front_matter::FrontMatter;
use super::guidance::state_guidance;
use super::markdown::push_notes;
use super::rows::{DecisionRow, FrontierRow, InitiativeHeader};
use crate::format::{clamp_gist, flatten_lines};
use crate::model::InitiativeState;

/// Everything `wayfind map` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapView {
    /// The initiative being mapped.
    pub initiative: InitiativeHeader,
    /// Tickets available right now, in identifier order.
    pub frontier: Vec<FrontierRow>,
    /// Where the initiative stands, used for guidance when the frontier is
    /// empty.
    pub state: InitiativeState,
    /// Settled tickets, in decision order.
    pub decisions: Vec<DecisionRow>,
    /// Questions the map has not answered yet.
    pub fog: Vec<String>,
    /// Questions the map has deliberately dropped.
    pub exclusions: Vec<String>,
}

/// Render `wayfind map`.
pub fn render_map(model: &MapView) -> String {
    let header = &model.initiative;
    let mut out = FrontMatter::new("map")
        .number("initiative_id", header.id.get())
        .text("name", &header.name)
        .text("status", header.status.as_str())
        .render();
    out.push('\n');

    let _ = write!(
        out,
        "# {}\n\n## Destination\n\n{}\n\n",
        header.name, header.destination
    );
    if !header.notes.is_empty() {
        let _ = write!(out, "## Notes\n\n{}\n\n", header.notes);
    }

    out.push_str("## Frontier\n\n");
    if model.frontier.is_empty() {
        let _ = writeln!(out, "{}", state_guidance(&model.state, header.id));
    } else {
        for row in &model.frontier {
            let _ = writeln!(
                out,
                "- [{}] {} ({})",
                row.id,
                flatten_lines(&row.title),
                row.ticket_type
            );
        }
    }

    out.push_str(
        "\n## Decisions so far\n\nGists are clamped. Run `wayfind ticket ID` for the full decision.\n\n",
    );
    for row in &model.decisions {
        let _ = writeln!(
            out,
            "- [{}] {} — {}",
            row.ticket_id,
            flatten_lines(&row.title),
            clamp_gist(&row.gist)
        );
    }

    out.push_str("\n## Not yet specified\n\n");
    push_notes(&mut out, &model.fog);
    out.push_str("\n## Out of scope\n\n");
    push_notes(&mut out, &model.exclusions);
    out
}
