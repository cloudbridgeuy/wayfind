//! The dependency diagram `wayfind tree` draws.
//!
//! The drawing is a commit graph read sideways. Each ticket occupies a lane;
//! when a ticket is drawn, its lane is replaced by one lane per blocker, so the
//! lanes fan out downward toward the tickets everything else waits on. A ticket
//! that several others wait on appears once, in the lane it was first given.
//!
//! Rows are ordered by how deep the ticket sits — the longest chain of tickets
//! waiting above it — and then by identifier, descending. Depth first means a
//! ticket is always drawn before anything it waits on, which is what keeps a
//! lane open from the row that opens it to the row that closes it.
//!
//! This is the one output where layout is the meaning, so it is built and
//! tested as a whole picture rather than as fields.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::format::flatten_lines;
use crate::id::TicketId;
use crate::model::{Dependency, Ticket, TicketStatusLabel, TicketType};

/// One ticket, as the diagram draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// The ticket's identifier.
    pub id: TicketId,
    /// Its title.
    pub title: String,
    /// Where it stands.
    pub status: TicketStatusLabel,
    /// Its kind, printed after the status.
    pub ticket_type: TicketType,
    /// The tickets it waits on, in identifier order.
    pub blockers: Vec<TicketId>,
}

impl TreeRow {
    /// The mark that shows where this ticket stands at a glance.
    pub fn marker(&self) -> char {
        match self.status {
            TicketStatusLabel::Resolved => '✓',
            TicketStatusLabel::Claimed => '▶',
            TicketStatusLabel::Open | TicketStatusLabel::Excluded => '*',
        }
    }
}

/// Everything `wayfind tree` draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeView {
    /// The initiative's name, printed as the heading.
    pub name: String,
    /// Every ticket, already in drawing order.
    pub rows: Vec<TreeRow>,
}

impl TreeView {
    /// Build the diagram's rows from an initiative's tickets and edges.
    ///
    /// Edges that name a ticket outside this initiative are dropped rather than
    /// refused: the diagram draws what this map holds, and a dangling edge is a
    /// question for the store, not something to stop a reader over.
    pub fn new(name: impl Into<String>, tickets: &[Ticket], dependencies: &[Dependency]) -> Self {
        let mut blockers: BTreeMap<TicketId, Vec<TicketId>> = BTreeMap::new();
        let mut dependents: BTreeMap<TicketId, Vec<TicketId>> = BTreeMap::new();
        let known: Vec<TicketId> = tickets.iter().map(|ticket| ticket.id).collect();
        for edge in dependencies {
            let (ticket, blocker) = (edge.ticket_id(), edge.blocker_id());
            if !known.contains(&ticket) || !known.contains(&blocker) {
                continue;
            }
            let waiting = blockers.entry(ticket).or_default();
            if !waiting.contains(&blocker) {
                waiting.push(blocker);
            }
            let above = dependents.entry(blocker).or_default();
            if !above.contains(&ticket) {
                above.push(ticket);
            }
        }
        for waiting in blockers.values_mut() {
            waiting.sort_unstable();
        }

        let depths = depths(&known, &dependents);
        let mut rows: Vec<TreeRow> = tickets
            .iter()
            .map(|ticket| TreeRow {
                id: ticket.id,
                title: ticket.title.clone(),
                status: ticket.state.label(),
                ticket_type: ticket.ticket_type,
                blockers: blockers.get(&ticket.id).cloned().unwrap_or_default(),
            })
            .collect();
        rows.sort_by(|left, right| {
            depths
                .get(&left.id)
                .copied()
                .unwrap_or(0)
                .cmp(&depths.get(&right.id).copied().unwrap_or(0))
                .then_with(|| right.id.cmp(&left.id))
        });

        Self {
            name: name.into(),
            rows,
        }
    }
}

/// How deep each ticket sits: the longest chain of tickets waiting above it.
///
/// The walk follows dependents rather than blockers, so a ticket's depth is one
/// more than the deepest ticket that waits on it. Cycles are impossible by
/// construction, and the visited set makes the walk terminate anyway rather
/// than trusting that.
fn depths(
    tickets: &[TicketId],
    dependents: &BTreeMap<TicketId, Vec<TicketId>>,
) -> BTreeMap<TicketId, u32> {
    let mut settled: BTreeMap<TicketId, u32> = BTreeMap::new();
    for &ticket in tickets {
        let mut visiting = Vec::new();
        depth_of(ticket, dependents, &mut settled, &mut visiting);
    }
    settled
}

fn depth_of(
    ticket: TicketId,
    dependents: &BTreeMap<TicketId, Vec<TicketId>>,
    settled: &mut BTreeMap<TicketId, u32>,
    visiting: &mut Vec<TicketId>,
) -> u32 {
    if let Some(&known) = settled.get(&ticket) {
        return known;
    }
    if visiting.contains(&ticket) {
        return 0;
    }
    visiting.push(ticket);
    let depth = dependents
        .get(&ticket)
        .map(|above| {
            above
                .iter()
                .map(|&waiting| depth_of(waiting, dependents, settled, visiting) + 1)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    visiting.pop();
    settled.insert(ticket, depth);
    depth
}

/// Draw the diagram.
pub fn render_tree(model: &TreeView) -> String {
    let mut out = format!("# {}\n\n", model.name);
    let mut lanes: Vec<TicketId> = Vec::new();

    for row in &model.rows {
        let slot = match lanes.iter().position(|&open| open == row.id) {
            Some(index) => index,
            None => {
                lanes.insert(0, row.id);
                0
            }
        };
        let prefix = "| ".repeat(slot);
        let _ = writeln!(
            out,
            "{prefix}{} [{}] {} · {} · {}",
            row.marker(),
            row.id,
            flatten_lines(&row.title),
            row.status,
            row.ticket_type
        );
        if !row.blockers.is_empty() {
            let joined = row
                .blockers
                .iter()
                .map(|id| id.get().to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fork = if row.blockers.len() == 1 {
                "|  -> "
            } else {
                "|\\ -> "
            };
            let _ = writeln!(out, "{prefix}{fork}[{joined}]");
        }
        lanes.splice(slot..=slot, row.blockers.iter().copied());
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{render_tree, TreeRow, TreeView};
    use crate::id::{InitiativeId, SessionId, TicketId};
    use crate::model::{Dependency, Ticket, TicketState, TicketStatusLabel, TicketType};
    use crate::time::Timestamp;

    fn ticket_id(value: i64) -> TicketId {
        TicketId::new(value).unwrap()
    }

    fn moment() -> Timestamp {
        Timestamp::from_str("2026-08-02 13:45:09").unwrap()
    }

    fn ticket(value: i64, title: &str, state: TicketState) -> Ticket {
        Ticket {
            id: ticket_id(value),
            initiative_id: InitiativeId::new(1).unwrap(),
            title: title.to_string(),
            ticket_type: TicketType::Task,
            question: String::new(),
            state,
            created_at: moment(),
        }
    }

    fn edge(ticket: i64, blocker: i64) -> Dependency {
        Dependency::new(ticket_id(ticket), ticket_id(blocker)).unwrap()
    }

    fn row(value: i64, title: &str, status: TicketStatusLabel, blockers: Vec<i64>) -> TreeRow {
        TreeRow {
            id: ticket_id(value),
            title: title.to_string(),
            status,
            ticket_type: TicketType::Task,
            blockers: blockers.into_iter().map(ticket_id).collect(),
        }
    }

    #[test]
    fn a_ticket_with_no_edges_is_one_row_in_the_first_lane() {
        let model = TreeView {
            name: "Cache the map".to_string(),
            rows: vec![row(1, "Alone", TicketStatusLabel::Open, Vec::new())],
        };
        assert_eq!(
            render_tree(&model),
            "# Cache the map\n\n* [1] Alone · open · task\n"
        );
    }

    #[test]
    fn each_status_gets_its_own_mark() {
        let model = TreeView {
            name: "Marks".to_string(),
            rows: vec![
                row(3, "Done", TicketStatusLabel::Resolved, Vec::new()),
                row(2, "Held", TicketStatusLabel::Claimed, Vec::new()),
                row(1, "Free", TicketStatusLabel::Open, Vec::new()),
            ],
        };
        let rendered = render_tree(&model);
        assert!(rendered.contains("✓ [3] Done · resolved · task\n"));
        assert!(rendered.contains("▶ [2] Held · claimed · task\n"));
        assert!(rendered.contains("* [1] Free · open · task\n"));
    }

    #[test]
    fn a_chain_keeps_one_lane_open_and_points_down_it() {
        // 3 waits on 2, and 2 waits on 1. One lane carries the whole chain.
        let model = TreeView {
            name: "Chain".to_string(),
            rows: vec![
                row(3, "Third", TicketStatusLabel::Open, vec![2]),
                row(2, "Second", TicketStatusLabel::Open, vec![1]),
                row(1, "First", TicketStatusLabel::Resolved, Vec::new()),
            ],
        };
        assert_eq!(
            render_tree(&model),
            concat!(
                "# Chain\n\n",
                "* [3] Third · open · task\n",
                "|  -> [2]\n",
                "* [2] Second · open · task\n",
                "|  -> [1]\n",
                "✓ [1] First · resolved · task\n",
            )
        );
    }

    #[test]
    fn a_ticket_waiting_on_two_forks_into_two_lanes() {
        let model = TreeView {
            name: "Fork".to_string(),
            rows: vec![
                row(3, "Third", TicketStatusLabel::Open, vec![1, 2]),
                row(1, "First", TicketStatusLabel::Open, Vec::new()),
                row(2, "Second", TicketStatusLabel::Open, Vec::new()),
            ],
        };
        assert_eq!(
            render_tree(&model),
            concat!(
                "# Fork\n\n",
                "* [3] Third · open · task\n",
                "|\\ -> [1,2]\n",
                "* [1] First · open · task\n",
                // Lane one closed when ticket 1 was drawn, so ticket 2 moves
                // left into it.
                "* [2] Second · open · task\n",
            )
        );
    }

    #[test]
    fn a_ticket_that_never_opened_a_lane_opens_one_at_the_front() {
        let model = TreeView {
            name: "Two roots".to_string(),
            rows: vec![
                row(2, "Second", TicketStatusLabel::Open, vec![1]),
                row(4, "Fourth", TicketStatusLabel::Open, vec![3]),
                row(1, "First", TicketStatusLabel::Open, Vec::new()),
                row(3, "Third", TicketStatusLabel::Open, Vec::new()),
            ],
        };
        assert_eq!(
            render_tree(&model),
            concat!(
                "# Two roots\n\n",
                "* [2] Second · open · task\n",
                "|  -> [1]\n",
                // Ticket 4 has no lane yet, so it takes a new one at the front
                // and pushes ticket 1's lane right.
                "* [4] Fourth · open · task\n",
                "|  -> [3]\n",
                "| * [1] First · open · task\n",
                "* [3] Third · open · task\n",
            )
        );
    }

    #[test]
    fn a_title_on_two_lines_stays_on_one_row() {
        let model = TreeView {
            name: "Wrapped".to_string(),
            rows: vec![row(1, "Two\nlines", TicketStatusLabel::Open, Vec::new())],
        };
        assert!(render_tree(&model).contains("* [1] Two lines · open · task\n"));
    }

    #[test]
    fn an_empty_initiative_draws_only_its_heading() {
        let model = TreeView {
            name: "Empty".to_string(),
            rows: Vec::new(),
        };
        assert_eq!(render_tree(&model), "# Empty\n\n");
    }

    // -- ordering ----------------------------------------------------------

    #[test]
    fn rows_are_ordered_by_depth_and_then_by_identifier_descending() {
        // 3 waits on 2, 2 waits on 1, and 4 waits on nothing.
        let tickets = vec![
            ticket(1, "First", TicketState::Open),
            ticket(2, "Second", TicketState::Open),
            ticket(3, "Third", TicketState::Open),
            ticket(4, "Fourth", TicketState::Open),
        ];
        let model = TreeView::new("Depths", &tickets, &[edge(3, 2), edge(2, 1)]);
        let order: Vec<i64> = model.rows.iter().map(|row| row.id.get()).collect();
        assert_eq!(order, vec![4, 3, 2, 1]);
    }

    #[test]
    fn depth_follows_the_longest_chain_rather_than_the_first_one_found() {
        // 1 is waited on directly by 4, and through 2 and 3 as well. The long
        // way round is what decides where it is drawn.
        let tickets = vec![
            ticket(1, "First", TicketState::Open),
            ticket(2, "Second", TicketState::Open),
            ticket(3, "Third", TicketState::Open),
            ticket(4, "Fourth", TicketState::Open),
        ];
        let model = TreeView::new("Longest", &tickets, &[edge(4, 1), edge(2, 1), edge(3, 2)]);
        let order: Vec<i64> = model.rows.iter().map(|row| row.id.get()).collect();
        assert_eq!(order, vec![4, 3, 2, 1]);
    }

    #[test]
    fn blockers_of_a_row_come_out_in_identifier_order() {
        let tickets = vec![
            ticket(1, "First", TicketState::Open),
            ticket(2, "Second", TicketState::Open),
            ticket(3, "Third", TicketState::Open),
        ];
        let model = TreeView::new("Sorted", &tickets, &[edge(3, 2), edge(3, 1)]);
        let third = model
            .rows
            .iter()
            .find(|row| row.id == ticket_id(3))
            .expect("ticket three");
        assert_eq!(third.blockers, vec![ticket_id(1), ticket_id(2)]);
    }

    #[test]
    fn an_edge_naming_a_ticket_off_this_map_is_left_out_of_the_drawing() {
        let tickets = vec![ticket(1, "First", TicketState::Open)];
        let model = TreeView::new("Dangling", &tickets, &[edge(1, 99)]);
        assert_eq!(model.rows.len(), 1);
        assert!(model.rows[0].blockers.is_empty());
        assert_eq!(
            render_tree(&model),
            "# Dangling\n\n* [1] First · open · task\n"
        );
    }

    #[test]
    fn a_claimed_ticket_is_drawn_with_the_holding_mark() {
        let tickets = vec![ticket(
            1,
            "Held",
            TicketState::Claimed {
                claimant: SessionId::new("session-a").unwrap(),
                claimed_at: moment(),
            },
        )];
        let model = TreeView::new("Held", &tickets, &[]);
        assert!(render_tree(&model).contains("▶ [1] Held · claimed · task\n"));
    }
}
