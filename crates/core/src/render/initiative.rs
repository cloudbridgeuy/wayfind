//! The `initiative` document: what `initiative create` and `initiative show`
//! print; and the `initiative-list` document `initiative list` prints.

use crate::id::{ProjectKey, RecordId, SnapshotOrdinal};
use crate::record::Initiative;
use crate::render::FrontMatter;

/// Render one initiative at a given head snapshot.
///
/// The front matter carries the destination node's full hash, so a script can
/// address the record exactly; the body carries only the abbreviation, since
/// an operator reading the page does not need the other 56 characters.
pub fn initiative_document(
    initiative: &Initiative,
    head: SnapshotOrdinal,
    destination: &RecordId,
) -> String {
    let mut out = FrontMatter::new("initiative")
        .number("initiative", initiative.id.get())
        .text("name", &initiative.name)
        .text("destination", &initiative.destination)
        .text("head", head.to_string())
        .text("created", initiative.created_at.to_string())
        .text("destination_node", destination.to_string())
        .render();

    out.push('\n');
    out.push_str(&format!("# {}\n\n", initiative.name));
    out.push_str(&format!("## Destination\n\n{}\n\n", initiative.destination));
    out.push_str(&format!(
        "Destination node: {}\n",
        destination.abbreviated()
    ));

    out
}

/// Render every initiative of a project.
pub fn initiative_list_document(project: &ProjectKey, initiatives: &[Initiative]) -> String {
    let mut out = FrontMatter::new("initiative-list")
        .text("project", project.as_str())
        .number("count", initiatives.len() as i64)
        .render();

    out.push('\n');
    for initiative in initiatives {
        out.push_str(&format!("## {}\n\n", initiative.name));
        out.push_str(&format!("- initiative: {}\n", initiative.id.get()));
        out.push_str(&format!("- destination: {}\n", initiative.destination));
        out.push_str(&format!("- created: {}\n\n", initiative.created_at));
    }

    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{initiative_document, initiative_list_document};
    use crate::id::{InitiativeId, ProjectKey, RecordId, SnapshotOrdinal};
    use crate::record::Initiative;
    use crate::time::Timestamp;

    fn initiative() -> Initiative {
        Initiative {
            id: InitiativeId::new(1).unwrap(),
            project: ProjectKey::new("/repo").unwrap(),
            name: "Ship v2".into(),
            destination: "wayfind v2 in daily use".into(),
            notes: None,
            created_at: Timestamp::parse_rfc3339("2026-08-03T00:00:00Z").unwrap(),
        }
    }

    #[test]
    fn front_matter_carries_the_full_hash_and_the_body_the_abbreviation() {
        let i = initiative();
        let id = RecordId::from_str(
            "R-0000000000000000000000000000000000000000000000000000000000000abc",
        )
        .unwrap();

        let out = initiative_document(&i, SnapshotOrdinal::new(1).unwrap(), &id);

        assert!(out.contains(&format!("destination_node = \"{}\"", id)));
        assert!(out.contains(&id.abbreviated()));
    }

    #[test]
    fn the_list_document_carries_the_project_the_count_and_one_section_per_initiative() {
        let project = ProjectKey::new("/repo").unwrap();
        let out = initiative_list_document(&project, &[initiative()]);

        assert!(out.contains("kind = \"initiative-list\""));
        assert!(out.contains("project = \"/repo\""));
        assert!(out.contains("count = 1"));
        assert!(out.contains("## Ship v2"));

        let empty = initiative_list_document(&project, &[]);
        assert!(empty.contains("count = 0"));
    }
}
