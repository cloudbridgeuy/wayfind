//! The `initiative` document: what `initiative create` and `initiative show`
//! print.

use crate::id::{RecordId, SnapshotOrdinal};
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::initiative_document;
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
}
