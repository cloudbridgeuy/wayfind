//! The dependency graph.
//!
//! Wayfind's graph answers two questions and nothing else: which tickets can be
//! picked up right now, and whether a proposed edge would make a ticket wait on
//! itself. Both answers are computed here, in the core, from records the shell
//! read — never pushed down into a recursive query that only one store can run.
//!
//! One rule decides both answers: a ticket is available when it is open and
//! every ticket it waits on carries a decision. "Carries a decision" means
//! resolved and nothing else. A blocker that was ruled out of scope still
//! blocks, which is the Bash script's behaviour and is the safer reading: an
//! excluded blocker is a question somebody dropped, not one somebody answered.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::id::TicketId;
use crate::model::{Dependency, FrontierTicket, Ticket, TicketState};

/// The blockers of each ticket, keyed by the ticket that waits.
///
/// Built once and read many times: classifying an initiative walks every
/// ticket, and doing that against a flat edge list would be quadratic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    blockers: BTreeMap<TicketId, BTreeSet<TicketId>>,
}

impl DependencyGraph {
    /// Build the graph from an edge list.
    pub fn new(edges: &[Dependency]) -> Self {
        let mut blockers: BTreeMap<TicketId, BTreeSet<TicketId>> = BTreeMap::new();
        for edge in edges {
            blockers
                .entry(edge.ticket_id())
                .or_default()
                .insert(edge.blocker_id());
        }
        Self { blockers }
    }

    /// The tickets one ticket waits on, in ascending identifier order.
    pub fn blockers_of(&self, ticket_id: TicketId) -> impl Iterator<Item = TicketId> + '_ {
        self.blockers
            .get(&ticket_id)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    /// Whether an edge is already in the graph.
    pub fn holds(&self, edge: Dependency) -> bool {
        self.blockers
            .get(&edge.ticket_id())
            .is_some_and(|set| set.contains(&edge.blocker_id()))
    }

    /// Every ticket that waits on something, in ascending identifier order.
    pub fn waiting_tickets(&self) -> impl Iterator<Item = TicketId> + '_ {
        self.blockers.keys().copied()
    }

    /// The shortest path of blockers from `from` to `to`, if one exists.
    ///
    /// The path starts at `from` and ends at `to`. Adjacency is walked in
    /// ascending identifier order and the search is breadth-first, so the same
    /// graph always reports the same path.
    fn shortest_path(&self, from: TicketId, to: TicketId) -> Option<Vec<TicketId>> {
        if from == to {
            return Some(vec![from]);
        }
        let mut came_from: BTreeMap<TicketId, TicketId> = BTreeMap::new();
        let mut seen: BTreeSet<TicketId> = BTreeSet::from([from]);
        let mut queue: VecDeque<TicketId> = VecDeque::from([from]);

        while let Some(current) = queue.pop_front() {
            for next in self.blockers_of(current) {
                if !seen.insert(next) {
                    continue;
                }
                came_from.insert(next, current);
                if next == to {
                    return Some(rebuild_path(&came_from, from, to));
                }
                queue.push_back(next);
            }
        }
        None
    }
}

/// Walk `came_from` backwards from `to` and hand back the forward path.
fn rebuild_path(
    came_from: &BTreeMap<TicketId, TicketId>,
    from: TicketId,
    to: TicketId,
) -> Vec<TicketId> {
    let mut path = vec![to];
    let mut current = to;
    while current != from {
        match came_from.get(&current) {
            Some(previous) => {
                path.push(*previous);
                current = *previous;
            }
            // Unreachable while `came_from` is built by the search above, which
            // records a predecessor for every node it enqueues.
            None => break,
        }
    }
    path.reverse();
    path
}

/// Whether a ticket can be picked up, given the state of everything it waits on.
///
/// A ticket that waits on a blocker the caller did not supply is treated as
/// unblocked by that edge. The store's foreign keys make that impossible in
/// practice, and matching the Bash script's inner join is better than inventing
/// a stricter rule here.
fn is_available(
    ticket: &Ticket,
    graph: &DependencyGraph,
    states: &BTreeMap<TicketId, &TicketState>,
) -> bool {
    if !matches!(ticket.state, TicketState::Open) {
        return false;
    }
    graph.blockers_of(ticket.id).all(|blocker| {
        states
            .get(&blocker)
            .is_none_or(|state| matches!(state, TicketState::Resolved { .. }))
    })
}

/// Every ticket that can be picked up right now, in ascending identifier order.
pub fn frontier(tickets: &[Ticket], dependencies: &[Dependency]) -> Vec<FrontierTicket> {
    let graph = DependencyGraph::new(dependencies);
    let states: BTreeMap<TicketId, &TicketState> =
        tickets.iter().map(|t| (t.id, &t.state)).collect();

    let mut available: Vec<&Ticket> = tickets
        .iter()
        .filter(|ticket| is_available(ticket, &graph, &states))
        .collect();
    available.sort_by_key(|ticket| ticket.id);
    available.iter().map(|t| t.to_frontier_entry()).collect()
}

/// The path that a proposed edge would close, if it would close one.
///
/// The returned path starts and ends at the waiting ticket, so an operator can
/// read the whole loop rather than being told only that one exists.
pub fn cycle_from(edges: &[Dependency], candidate: Dependency) -> Option<Vec<TicketId>> {
    let graph = DependencyGraph::new(edges);
    let mut path = graph.shortest_path(candidate.blocker_id(), candidate.ticket_id())?;
    path.insert(0, candidate.ticket_id());
    Some(path)
}

/// Whether a proposed edge would make a ticket wait on itself.
///
/// An edge that is already in the graph closes nothing new, so this reports
/// `false` for a duplicate. [`Dependency`] already refuses a self edge, so the
/// only cycles reachable here run through at least one other ticket.
pub fn would_create_cycle(edges: &[Dependency], candidate: Dependency) -> bool {
    cycle_from(edges, candidate).is_some()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{cycle_from, frontier, would_create_cycle, DependencyGraph};
    use crate::id::{InitiativeId, SessionId, TicketId};
    use crate::model::{Dependency, Ticket, TicketState, TicketType};
    use crate::time::Timestamp;

    fn id(value: i64) -> TicketId {
        TicketId::new(value).unwrap()
    }

    fn moment() -> Timestamp {
        Timestamp::from_str("2026-08-02 13:45:09").unwrap()
    }

    fn ticket(value: i64, state: TicketState) -> Ticket {
        Ticket {
            id: id(value),
            initiative_id: InitiativeId::new(1).unwrap(),
            title: format!("ticket {value}"),
            ticket_type: TicketType::Task,
            question: String::new(),
            state,
            created_at: moment(),
        }
    }

    fn resolved() -> TicketState {
        TicketState::Resolved {
            resolution: "settled".to_string(),
            resolved_at: moment(),
            amended_at: None,
        }
    }

    fn claimed() -> TicketState {
        TicketState::Claimed {
            claimant: SessionId::new("session-1").unwrap(),
            claimed_at: moment(),
        }
    }

    fn edge(ticket_id: i64, blocker_id: i64) -> Dependency {
        Dependency::new(id(ticket_id), id(blocker_id)).unwrap()
    }

    fn ids(entries: &[crate::model::FrontierTicket]) -> Vec<i64> {
        entries.iter().map(|entry| entry.id.get()).collect()
    }

    #[test]
    fn an_unblocked_open_ticket_is_available() {
        let tickets = vec![ticket(1, TicketState::Open)];
        assert_eq!(ids(&frontier(&tickets, &[])), vec![1]);
    }

    #[test]
    fn only_open_tickets_reach_the_frontier() {
        let tickets = vec![
            ticket(1, TicketState::Open),
            ticket(2, claimed()),
            ticket(3, resolved()),
            ticket(4, TicketState::Excluded),
        ];
        assert_eq!(ids(&frontier(&tickets, &[])), vec![1]);
    }

    #[test]
    fn the_frontier_is_ordered_by_ticket_id() {
        let tickets = vec![
            ticket(30, TicketState::Open),
            ticket(4, TicketState::Open),
            ticket(12, TicketState::Open),
        ];
        assert_eq!(ids(&frontier(&tickets, &[])), vec![4, 12, 30]);
    }

    #[test]
    fn a_ticket_waits_until_every_blocker_carries_a_decision() {
        let tickets = vec![
            ticket(1, resolved()),
            ticket(2, TicketState::Open),
            ticket(3, TicketState::Open),
        ];
        let edges = vec![edge(3, 1), edge(3, 2)];
        assert_eq!(ids(&frontier(&tickets, &edges)), vec![2]);

        let tickets = vec![
            ticket(1, resolved()),
            ticket(2, resolved()),
            ticket(3, TicketState::Open),
        ];
        assert_eq!(ids(&frontier(&tickets, &edges)), vec![3]);
    }

    #[test]
    fn an_excluded_blocker_still_blocks() {
        let tickets = vec![
            ticket(1, TicketState::Excluded),
            ticket(2, TicketState::Open),
        ];
        assert!(frontier(&tickets, &[edge(2, 1)]).is_empty());
    }

    #[test]
    fn an_unknown_blocker_does_not_block_as_the_bash_join_does_not() {
        let tickets = vec![ticket(2, TicketState::Open)];
        assert_eq!(ids(&frontier(&tickets, &[edge(2, 99)])), vec![2]);
    }

    #[test]
    fn a_duplicate_edge_closes_nothing() {
        let edges = vec![edge(2, 1)];
        assert!(!would_create_cycle(&edges, edge(2, 1)));
    }

    #[test]
    fn a_two_ticket_loop_is_a_cycle() {
        let edges = vec![edge(2, 1)];
        assert!(would_create_cycle(&edges, edge(1, 2)));
        assert_eq!(
            cycle_from(&edges, edge(1, 2)),
            Some(vec![id(1), id(2), id(1)])
        );
    }

    #[test]
    fn a_deep_loop_is_a_cycle_and_reports_the_whole_path() {
        let edges = vec![edge(2, 1), edge(3, 2), edge(4, 3)];
        assert!(would_create_cycle(&edges, edge(1, 4)));
        assert_eq!(
            cycle_from(&edges, edge(1, 4)),
            Some(vec![id(1), id(4), id(3), id(2), id(1)])
        );
    }

    #[test]
    fn a_diamond_is_not_a_cycle() {
        // 4 waits on 2 and 3; both wait on 1.
        let edges = vec![edge(2, 1), edge(3, 1), edge(4, 2), edge(4, 3)];
        assert!(!would_create_cycle(&edges, edge(4, 1)));
        let tickets = vec![
            ticket(1, resolved()),
            ticket(2, TicketState::Open),
            ticket(3, TicketState::Open),
            ticket(4, TicketState::Open),
        ];
        assert_eq!(ids(&frontier(&tickets, &edges)), vec![2, 3]);
    }

    #[test]
    fn a_disjoint_component_never_closes_a_loop() {
        let edges = vec![edge(2, 1), edge(4, 3)];
        assert!(!would_create_cycle(&edges, edge(1, 3)));
        assert!(!would_create_cycle(&edges, edge(3, 1)));
    }

    #[test]
    fn a_cycle_path_is_the_shortest_one_and_is_stable_across_runs() {
        // 1 is reachable from 5 by a long way and a short way.
        let edges = vec![edge(5, 4), edge(4, 3), edge(3, 1), edge(5, 1)];
        let first = cycle_from(&edges, edge(1, 5));
        assert_eq!(first, Some(vec![id(1), id(5), id(1)]));
        assert_eq!(first, cycle_from(&edges, edge(1, 5)));
    }

    #[test]
    fn the_graph_reports_its_own_edges() {
        let graph = DependencyGraph::new(&[edge(3, 2), edge(3, 1), edge(3, 1)]);
        assert_eq!(
            graph.blockers_of(id(3)).collect::<Vec<_>>(),
            vec![id(1), id(2)]
        );
        assert_eq!(graph.blockers_of(id(1)).count(), 0);
        assert!(graph.holds(edge(3, 1)));
        assert!(!graph.holds(edge(1, 3)));
        assert_eq!(graph.waiting_tickets().collect::<Vec<_>>(), vec![id(3)]);
    }
}
