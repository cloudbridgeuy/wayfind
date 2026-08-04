//! `wayfind dump --csv` records, and the two one-line reports.

use crate::error::{Error, Result};
use crate::format::flatten_lines;
use crate::id::{InitiativeId, ProjectKey, TicketId};
use crate::model::{TicketStatusLabel, TicketType};

/// One record of `wayfind dump --csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub ticket_type: TicketType,
    /// Where it stands.
    pub status: TicketStatusLabel,
    /// The question it asks, flattened to one line.
    pub question: String,
    /// The decision, flattened to one line, empty when there is none.
    pub resolution: String,
}

/// The header row of `wayfind dump --csv`.
pub const DUMP_HEADER: [&str; 6] = ["id", "title", "type", "status", "question", "resolution"];

/// Render `wayfind dump --csv`.
///
/// Question and decision text is flattened to one line, so one ticket is one
/// record whatever the author wrote. Quoting is the CSV writer's job, so a
/// title holding a comma or a quotation mark survives the round trip.
pub fn render_csv(rows: &[DumpRow]) -> Result<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(DUMP_HEADER)
        .map_err(|error| csv_failed(&error))?;
    for row in rows {
        writer
            .write_record([
                row.id.get().to_string().as_str(),
                flatten_lines(&row.title).as_str(),
                row.ticket_type.as_str(),
                row.status.as_str(),
                flatten_lines(&row.question).as_str(),
                flatten_lines(&row.resolution).as_str(),
            ])
            .map_err(|error| csv_failed(&error))?;
    }
    let bytes = writer.into_inner().map_err(|error| csv_failed(&error))?;
    String::from_utf8(bytes).map_err(|error| csv_failed(&error))
}

fn csv_failed(error: &dyn std::fmt::Display) -> Error {
    Error::invalid_value("csv record", error.to_string())
}

/// Render what `wayfind init` reports.
pub fn render_init(database: &str, project_key: &ProjectKey) -> String {
    format!("initialized {database} for {project_key}\n")
}

/// Render what `wayfind initiative clear` reports.
pub fn render_initiative_cleared(initiative: InitiativeId) -> String {
    format!("initiative {initiative} is clear\n")
}
