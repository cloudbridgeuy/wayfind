//! What an initiative's map says, and how to read one without tearing.
//!
//! An initiative is classified from two things: the status an operator set
//! deliberately, and the shape of the ticket graph. The stored status wins only
//! when it is `clear`, because closing a map is a decision and nothing computed
//! should quietly reopen it. Everything else is derived, so a map that has gone
//! from blocked to ready needs no write to say so.
//!
//! Reading is the other half. Nine separate reads make one map, and an agent
//! resolving a ticket halfway through would produce a map that never existed —
//! a frontier from before the write beside decisions from after it. So reads are
//! stabilized: take the revision, read the records, take the revision again, and
//! start over if it moved. The retry budget arrives as data, and the core never
//! reads a clock and never sleeps.

use std::num::NonZeroU32;

use crate::error::{Error, Result};
use crate::graph::frontier;
use crate::id::{InitiativeId, TicketId};
use crate::model::{
    BlockedReason, Dependency, FrontierTicket, Initiative, InitiativeState, NonEmptyVec,
    PersistedInitiativeStatus, Session, Ticket, TicketState,
};
use crate::storage::{Consistency, EntityReader, InitiativeRevision, StorageResult};

/// How many tickets of an initiative sit where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TicketCounts {
    /// Every ticket, whatever its state.
    pub total: u32,
    /// Tickets nobody holds and nobody has settled.
    pub open: u32,
    /// Tickets a session is holding.
    pub claimed: u32,
}

impl TicketCounts {
    /// Count the states of a ticket list.
    pub fn of(tickets: &[Ticket]) -> Self {
        let mut counts = TicketCounts {
            total: u32::try_from(tickets.len()).unwrap_or(u32::MAX),
            ..TicketCounts::default()
        };
        for ticket in tickets {
            match ticket.state {
                TicketState::Open => counts.open = counts.open.saturating_add(1),
                TicketState::Claimed { .. } => counts.claimed = counts.claimed.saturating_add(1),
                TicketState::Resolved { .. } | TicketState::Excluded => {}
            }
        }
        counts
    }

    /// Whether work is still outstanding.
    pub fn has_outstanding_work(self) -> bool {
        self.open + self.claimed > 0
    }
}

/// One initiative and everything the decision functions read about it, taken at
/// a single revision.
///
/// The parts are checked against each other on the way in, so nothing
/// downstream has to ask whether a ticket really belongs to this initiative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiativeView {
    initiative: Initiative,
    tickets: Vec<Ticket>,
    dependencies: Vec<Dependency>,
    sessions: Vec<Session>,
    revision: InitiativeRevision,
}

impl InitiativeView {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "initiative view";

    /// Assemble a view, rejecting parts that do not belong together.
    ///
    /// A ticket from another initiative, an edge naming a ticket that is not
    /// here, or a session bound elsewhere all mean the store handed back
    /// something the domain says cannot exist.
    pub fn new(
        initiative: Initiative,
        mut tickets: Vec<Ticket>,
        dependencies: Vec<Dependency>,
        sessions: Vec<Session>,
        revision: InitiativeRevision,
    ) -> Result<Self> {
        let id = initiative.id;
        for ticket in &tickets {
            if ticket.initiative_id != id {
                return Err(Error::corrupt_data(
                    Self::FIELD,
                    format!(
                        "ticket {} belongs to initiative {}, not {id}",
                        ticket.id, ticket.initiative_id
                    ),
                ));
            }
        }
        tickets.sort_by_key(|ticket| ticket.id);

        for edge in &dependencies {
            for participant in [edge.ticket_id(), edge.blocker_id()] {
                if !tickets.iter().any(|ticket| ticket.id == participant) {
                    return Err(Error::corrupt_data(
                        Self::FIELD,
                        format!("dependency names ticket {participant}, which is not in initiative {id}"),
                    ));
                }
            }
        }

        for session in &sessions {
            if session.initiative_id != Some(id) {
                return Err(Error::corrupt_data(
                    Self::FIELD,
                    format!("session {} is not bound to initiative {id}", session.id),
                ));
            }
        }

        Ok(Self {
            initiative,
            tickets,
            dependencies,
            sessions,
            revision,
        })
    }

    /// The initiative record.
    pub fn initiative(&self) -> &Initiative {
        &self.initiative
    }

    /// The initiative's identifier.
    pub fn id(&self) -> InitiativeId {
        self.initiative.id
    }

    /// The revision every part of this view was read at.
    pub fn revision(&self) -> InitiativeRevision {
        self.revision
    }

    /// Every ticket, in ascending identifier order.
    pub fn tickets(&self) -> &[Ticket] {
        &self.tickets
    }

    /// Every dependency edge.
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// Every session bound to this initiative.
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// One ticket of this initiative.
    pub fn ticket(&self, id: TicketId) -> Option<&Ticket> {
        self.tickets.iter().find(|ticket| ticket.id == id)
    }

    /// How many tickets sit where.
    pub fn counts(&self) -> TicketCounts {
        TicketCounts::of(&self.tickets)
    }

    /// Every ticket that can be picked up right now.
    pub fn frontier(&self) -> Vec<FrontierTicket> {
        frontier(&self.tickets, &self.dependencies)
    }
}

/// What this initiative's map says about the work as a whole.
///
/// The order of the checks is the Bash script's, and the order matters: a
/// cleared map reports `clear` even when tickets remain, and an empty map
/// reports `charting` rather than `complete`.
pub fn classify_initiative(view: &InitiativeView) -> InitiativeState {
    let counts = view.counts();
    if view.initiative().status == PersistedInitiativeStatus::Clear {
        return InitiativeState::Clear;
    }
    if counts.total == 0 {
        return InitiativeState::Charting;
    }
    if !counts.has_outstanding_work() {
        return InitiativeState::Complete;
    }
    match NonEmptyVec::try_from(view.frontier()) {
        Ok(frontier) => InitiativeState::Ready { frontier },
        Err(_) if counts.claimed > 0 => {
            InitiativeState::Blocked(BlockedReason::ClaimsHoldFrontier {
                claimed: counts.claimed,
            })
        }
        Err(_) => InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked),
    }
}

/// The ticket `wayfind next` would hand out.
///
/// An empty frontier is an answer, not a failure: the caller reports the state
/// and its guidance instead.
pub fn next_ticket(state: &InitiativeState) -> Option<&FrontierTicket> {
    match state {
        InitiativeState::Ready { frontier } => Some(frontier.first()),
        InitiativeState::Charting
        | InitiativeState::Blocked(_)
        | InitiativeState::Complete
        | InitiativeState::Clear => None,
    }
}

// ---------------------------------------------------------------------------
// Stabilized reads
// ---------------------------------------------------------------------------

/// How hard to try for a view that did not move while it was being read.
///
/// Supplied as data rather than hard-coded, so a test can ask for exactly one
/// attempt and a command can ask for a few. There is no delay between attempts:
/// the core does not sleep, and a shell that wants to back off does so itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadPolicy {
    attempts: NonZeroU32,
}

impl ReadPolicy {
    /// The budget an ordinary command uses.
    pub const DEFAULT_ATTEMPTS: u32 = 3;

    /// Allow `attempts` reads before giving up.
    pub fn new(attempts: NonZeroU32) -> Self {
        Self { attempts }
    }

    /// Try exactly once. Useful when the caller wants to decide for itself.
    pub fn once() -> Self {
        Self {
            attempts: NonZeroU32::MIN,
        }
    }

    /// How many attempts this policy allows.
    pub fn attempts(self) -> u32 {
        self.attempts.get()
    }
}

impl Default for ReadPolicy {
    fn default() -> Self {
        Self {
            attempts: NonZeroU32::new(Self::DEFAULT_ATTEMPTS).unwrap_or(NonZeroU32::MIN),
        }
    }
}

/// The result of trying to read one initiative without tearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StableRead {
    /// The whole view was read at one revision.
    Stable(Box<InitiativeView>),
    /// There is no such initiative.
    Missing,
    /// The initiative changed under every attempt. Retrying is reasonable.
    Unsettled {
        /// How many attempts were spent.
        attempts: u32,
    },
}

impl StableRead {
    /// The view, if the read settled.
    pub fn view(&self) -> Option<&InitiativeView> {
        match self {
            StableRead::Stable(view) => Some(view),
            StableRead::Missing | StableRead::Unsettled { .. } => None,
        }
    }
}

/// Read one initiative's whole map at a single revision.
///
/// The shape is read-check-read: take the revision, strong-read every part,
/// take the revision again. Equal revisions mean nothing committed in between,
/// so the parts agree with each other. A changed revision means somebody wrote
/// while this was reading, and the whole read starts over.
pub fn read_stable_initiative(
    reader: &dyn EntityReader,
    id: InitiativeId,
    policy: ReadPolicy,
) -> StorageResult<StableRead> {
    for _ in 0..policy.attempts() {
        let before = reader.initiative_revision(id, Consistency::Strong)?;
        let Some(initiative) = reader.initiative(id, Consistency::Strong)? else {
            return Ok(StableRead::Missing);
        };
        let tickets = reader.tickets(id, Consistency::Strong)?;
        let dependencies = reader.dependencies(id, Consistency::Strong)?;
        let sessions = reader.sessions(id, Consistency::Strong)?;
        let after = reader.initiative_revision(id, Consistency::Strong)?;

        if before == after {
            let view = InitiativeView::new(initiative, tickets, dependencies, sessions, before)?;
            return Ok(StableRead::Stable(Box::new(view)));
        }
    }
    Ok(StableRead::Unsettled {
        attempts: policy.attempts(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::cell::RefCell;
    use std::str::FromStr;

    use super::{
        classify_initiative, next_ticket, read_stable_initiative, InitiativeView, ReadPolicy,
        StableRead, TicketCounts,
    };
    use crate::id::{InitiativeId, ProjectKey, SessionId, TicketId};
    use crate::model::{
        ActiveSessionState, AttachmentMetadata, AttachmentReference, BlockedReason, Decision,
        Dependency, FogNote, Initiative, InitiativeState, PersistedInitiativeStatus, Project,
        ScopeExclusion, Session, SessionState, Ticket, TicketState, TicketType,
    };
    use crate::storage::{Consistency, EntityReader, InitiativeRevision, StorageResult};
    use crate::time::Timestamp;

    fn moment() -> Timestamp {
        Timestamp::from_str("2026-08-02 13:45:09").unwrap()
    }

    fn initiative_id() -> InitiativeId {
        InitiativeId::new(1).unwrap()
    }

    fn initiative(status: PersistedInitiativeStatus) -> Initiative {
        Initiative {
            id: initiative_id(),
            project_key: ProjectKey::new("/Users/example/project").unwrap(),
            name: "Cache the map".to_string(),
            destination: "A map that loads instantly".to_string(),
            notes: String::new(),
            status,
            created_at: moment(),
        }
    }

    fn ticket(value: i64, state: TicketState) -> Ticket {
        Ticket {
            id: TicketId::new(value).unwrap(),
            initiative_id: initiative_id(),
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

    fn view(
        status: PersistedInitiativeStatus,
        tickets: Vec<Ticket>,
        edges: Vec<Dependency>,
    ) -> InitiativeView {
        InitiativeView::new(
            initiative(status),
            tickets,
            edges,
            Vec::new(),
            InitiativeRevision::INITIAL,
        )
        .unwrap()
    }

    fn edge(ticket_id: i64, blocker_id: i64) -> Dependency {
        Dependency::new(
            TicketId::new(ticket_id).unwrap(),
            TicketId::new(blocker_id).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn an_initiative_with_no_tickets_is_charting() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Charting,
            Vec::new(),
            Vec::new(),
        ));
        assert_eq!(state, InitiativeState::Charting);
        assert!(next_ticket(&state).is_none());
    }

    #[test]
    fn an_initiative_with_an_available_ticket_is_ready_and_lists_its_whole_frontier() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Working,
            vec![
                ticket(3, TicketState::Open),
                ticket(1, TicketState::Open),
                ticket(2, resolved()),
            ],
            Vec::new(),
        ));
        let InitiativeState::Ready { frontier } = &state else {
            panic!("expected ready, got {state:?}");
        };
        let ids: Vec<i64> = frontier.iter().map(|entry| entry.id.get()).collect();
        assert_eq!(ids, vec![1, 3]);
        assert_eq!(next_ticket(&state).map(|entry| entry.id.get()), Some(1));
    }

    #[test]
    fn claims_holding_the_frontier_report_how_many() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Working,
            vec![ticket(1, claimed()), ticket(2, claimed())],
            Vec::new(),
        ));
        assert_eq!(
            state,
            InitiativeState::Blocked(BlockedReason::ClaimsHoldFrontier { claimed: 2 })
        );
        assert!(next_ticket(&state).is_none());
    }

    #[test]
    fn an_entirely_blocked_graph_says_so_instead_of_blaming_claims() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Working,
            vec![
                ticket(1, TicketState::Excluded),
                ticket(2, TicketState::Open),
            ],
            vec![edge(2, 1)],
        ));
        assert_eq!(
            state,
            InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked)
        );
    }

    #[test]
    fn an_initiative_with_every_ticket_settled_is_complete() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Working,
            vec![ticket(1, resolved()), ticket(2, TicketState::Excluded)],
            Vec::new(),
        ));
        assert_eq!(state, InitiativeState::Complete);
    }

    #[test]
    fn a_cleared_status_wins_over_everything_computed() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Clear,
            vec![ticket(1, TicketState::Open)],
            Vec::new(),
        ));
        assert_eq!(state, InitiativeState::Clear);
        assert!(next_ticket(&state).is_none());
    }

    #[test]
    fn an_empty_frontier_classifies_without_failing() {
        let state = classify_initiative(&view(
            PersistedInitiativeStatus::Working,
            vec![ticket(1, claimed())],
            Vec::new(),
        ));
        assert!(matches!(state, InitiativeState::Blocked(_)));
    }

    #[test]
    fn counts_report_each_state_separately() {
        let counts = TicketCounts::of(&[
            ticket(1, TicketState::Open),
            ticket(2, claimed()),
            ticket(3, resolved()),
            ticket(4, TicketState::Excluded),
        ]);
        assert_eq!(counts.total, 4);
        assert_eq!(counts.open, 1);
        assert_eq!(counts.claimed, 1);
        assert!(counts.has_outstanding_work());
        assert!(!TicketCounts::of(&[ticket(1, resolved())]).has_outstanding_work());
    }

    #[test]
    fn a_view_refuses_parts_that_do_not_belong_together() {
        let mut stranger = ticket(9, TicketState::Open);
        stranger.initiative_id = InitiativeId::new(2).unwrap();
        assert!(InitiativeView::new(
            initiative(PersistedInitiativeStatus::Working),
            vec![stranger],
            Vec::new(),
            Vec::new(),
            InitiativeRevision::INITIAL,
        )
        .is_err());

        assert!(InitiativeView::new(
            initiative(PersistedInitiativeStatus::Working),
            vec![ticket(1, TicketState::Open)],
            vec![edge(1, 7)],
            Vec::new(),
            InitiativeRevision::INITIAL,
        )
        .is_err());

        let stranger_session = Session {
            id: SessionId::new("session-1").unwrap(),
            project_key: ProjectKey::new("/Users/example/project").unwrap(),
            initiative_id: Some(InitiativeId::new(2).unwrap()),
            state: SessionState::Active(ActiveSessionState::Ready),
            resolved_non_research_count: 0,
            started_at: moment(),
            last_seen_at: moment(),
        };
        assert!(InitiativeView::new(
            initiative(PersistedInitiativeStatus::Working),
            Vec::new(),
            Vec::new(),
            vec![stranger_session],
            InitiativeRevision::INITIAL,
        )
        .is_err());
    }

    /// A reader whose revision moves for a fixed number of reads, so the
    /// stabilizing loop can be driven without a clock or a thread.
    struct MovingReader {
        moves_left: RefCell<u32>,
        reads: RefCell<u32>,
        exists: bool,
    }

    impl MovingReader {
        fn new(moves_left: u32, exists: bool) -> Self {
            Self {
                moves_left: RefCell::new(moves_left),
                reads: RefCell::new(0),
                exists,
            }
        }
    }

    impl EntityReader for MovingReader {
        fn project(&self, _key: &ProjectKey, _c: Consistency) -> StorageResult<Option<Project>> {
            Ok(None)
        }

        fn newest_initiative(
            &self,
            _selector: crate::storage::InitiativeSelector<'_>,
            _c: Consistency,
        ) -> StorageResult<Option<InitiativeId>> {
            Ok(None)
        }

        fn initiative_revision(
            &self,
            _id: InitiativeId,
            _c: Consistency,
        ) -> StorageResult<InitiativeRevision> {
            let mut reads = self.reads.borrow_mut();
            *reads += 1;
            // The closing read of an attempt is the even-numbered one.
            if reads.is_multiple_of(2) {
                let mut moves = self.moves_left.borrow_mut();
                if *moves > 0 {
                    *moves -= 1;
                    return Ok(InitiativeRevision::new(u64::from(*reads)));
                }
            }
            Ok(InitiativeRevision::INITIAL)
        }

        fn initiative(
            &self,
            _id: InitiativeId,
            _c: Consistency,
        ) -> StorageResult<Option<Initiative>> {
            Ok(self
                .exists
                .then(|| initiative(PersistedInitiativeStatus::Working)))
        }

        fn tickets(&self, _id: InitiativeId, _c: Consistency) -> StorageResult<Vec<Ticket>> {
            Ok(vec![ticket(1, TicketState::Open)])
        }

        fn dependencies(
            &self,
            _id: InitiativeId,
            _c: Consistency,
        ) -> StorageResult<Vec<Dependency>> {
            Ok(Vec::new())
        }

        fn sessions(&self, _id: InitiativeId, _c: Consistency) -> StorageResult<Vec<Session>> {
            Ok(Vec::new())
        }

        fn decisions(&self, _id: InitiativeId, _c: Consistency) -> StorageResult<Vec<Decision>> {
            Ok(Vec::new())
        }

        fn fog_notes(&self, _id: InitiativeId, _c: Consistency) -> StorageResult<Vec<FogNote>> {
            Ok(Vec::new())
        }

        fn scope_exclusions(
            &self,
            _id: InitiativeId,
            _c: Consistency,
        ) -> StorageResult<Vec<ScopeExclusion>> {
            Ok(Vec::new())
        }

        fn attachment_index(
            &self,
            _id: InitiativeId,
            _c: Consistency,
        ) -> StorageResult<Vec<AttachmentMetadata>> {
            Ok(Vec::new())
        }

        fn attachment_references(
            &self,
            _id: InitiativeId,
            _c: Consistency,
        ) -> StorageResult<Vec<AttachmentReference>> {
            Ok(Vec::new())
        }

        fn ticket(&self, _id: TicketId, _c: Consistency) -> StorageResult<Option<Ticket>> {
            Ok(None)
        }

        fn session(&self, _id: &SessionId, _c: Consistency) -> StorageResult<Option<Session>> {
            Ok(None)
        }

        fn attachment_metadata(
            &self,
            _id: crate::id::AttachmentId,
            _c: Consistency,
        ) -> StorageResult<Option<AttachmentMetadata>> {
            Ok(None)
        }
    }

    #[test]
    fn a_quiet_initiative_settles_on_the_first_attempt() {
        let reader = MovingReader::new(0, true);
        let read = read_stable_initiative(&reader, initiative_id(), ReadPolicy::default()).unwrap();
        let StableRead::Stable(view) = read else {
            panic!("expected a stable read");
        };
        assert_eq!(view.tickets().len(), 1);
        assert_eq!(*reader.reads.borrow(), 2);
    }

    #[test]
    fn a_busy_initiative_is_read_again_until_it_holds_still() {
        let reader = MovingReader::new(1, true);
        let read = read_stable_initiative(&reader, initiative_id(), ReadPolicy::default()).unwrap();
        assert!(matches!(read, StableRead::Stable(_)));
        assert_eq!(*reader.reads.borrow(), 4);
    }

    #[test]
    fn a_never_quiet_initiative_reports_unsettled_rather_than_a_torn_view() {
        let reader = MovingReader::new(u32::MAX, true);
        let read = read_stable_initiative(&reader, initiative_id(), ReadPolicy::once()).unwrap();
        assert_eq!(read, StableRead::Unsettled { attempts: 1 });
        assert!(read.view().is_none());
    }

    #[test]
    fn a_missing_initiative_is_reported_and_not_retried() {
        let reader = MovingReader::new(u32::MAX, false);
        let read = read_stable_initiative(&reader, initiative_id(), ReadPolicy::default()).unwrap();
        assert_eq!(read, StableRead::Missing);
        assert_eq!(*reader.reads.borrow(), 1);
    }
}
