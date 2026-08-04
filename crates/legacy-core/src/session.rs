//! Session rules.
//!
//! A session is one agent working one initiative. Two rules govern it, and both
//! exist to stop an agent from spreading itself thin rather than to protect the
//! data.
//!
//! **One ticket at a time.** A session holds at most one ticket. Finishing it is
//! what frees the session to take another.
//!
//! **One non-research resolution, ever.** A session may resolve any number of
//! research tickets, because reading does not commit anybody to anything. The
//! first time it settles a grilling, a prototype, or a task, that budget is
//! spent for the life of the session. The limit is permanent on purpose: a fresh
//! session is a fresh reading of the map, and the point is to force one.

use crate::command::TouchSession;
use crate::error::Result;
use crate::id::{InitiativeId, ProjectKey, SessionId, TicketId};
use crate::initiative::InitiativeView;
use crate::model::{ActiveSessionState, Session, SessionState, TicketType};
use crate::outcome::TouchSessionConflict;
use crate::time::Timestamp;

/// What a session may still take on.
///
/// Read once from a session record, so the claim and resolve rules cannot read
/// it two different ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionBudget {
    holding: Option<TicketId>,
    non_research_spent: bool,
}

impl SessionBudget {
    /// The budget of a session that has not been recorded yet.
    pub const FRESH: SessionBudget = SessionBudget {
        holding: None,
        non_research_spent: false,
    };

    /// Read a session's budget from its record.
    pub fn of(session: &Session) -> Self {
        Self {
            holding: match &session.state {
                SessionState::Active(ActiveSessionState::Holding { ticket_id }) => Some(*ticket_id),
                SessionState::Active(ActiveSessionState::Ready) | SessionState::Closed => None,
            },
            non_research_spent: session.has_spent_non_research_budget(),
        }
    }

    /// Read the budget of a session that may not exist yet.
    pub fn of_optional(session: Option<&Session>) -> Self {
        session.map_or(Self::FRESH, Self::of)
    }

    /// The ticket this session is holding, if any.
    pub fn holding(self) -> Option<TicketId> {
        self.holding
    }

    /// Whether this session has already settled a non-research ticket.
    pub fn non_research_spent(self) -> bool {
        self.non_research_spent
    }

    /// Whether this session may still take on a ticket of this kind.
    pub fn may_take(self, ticket_type: TicketType) -> bool {
        ticket_type.is_research() || !self.non_research_spent
    }
}

/// One session of a view, if it is there.
pub fn session_of<'a>(view: &'a InitiativeView, id: &SessionId) -> Option<&'a Session> {
    view.sessions().iter().find(|session| &session.id == id)
}

/// Every session still working this initiative, newest activity first.
///
/// The order is the Bash script's `ORDER BY last_seen_at DESC, id`, which is
/// total: two sessions last seen in the same second still sort the same way
/// every time.
pub fn active_sessions(view: &InitiativeView) -> Vec<&Session> {
    let mut active: Vec<&Session> = view
        .sessions()
        .iter()
        .filter(|session| matches!(session.state, SessionState::Active(_)))
        .collect();
    active.sort_by(|left, right| {
        right
            .last_seen_at
            .cmp(&left.last_seen_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    active
}

/// What the shell knows when it announces that a session is alive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchInput {
    /// The session announcing itself.
    pub session_id: SessionId,
    /// The project it is working in.
    pub project_key: ProjectKey,
    /// The initiative it is bound to, if the project has one.
    pub initiative_id: Option<InitiativeId>,
    /// The clock reading the shell took.
    pub now: Timestamp,
}

/// Build the command that records a session as alive.
///
/// A session never moves between projects or between initiatives. Binding is
/// decided once, when the session first appears, and a later command from a
/// different place is refused rather than quietly rebound — the operator wants a
/// new session identifier, not a session that forgot where it was.
pub fn prepare_touch_session(
    input: TouchInput,
    existing: Option<&Session>,
) -> std::result::Result<TouchSession, TouchSessionConflict> {
    if let Some(session) = existing {
        if session.project_key != input.project_key || session.initiative_id != input.initiative_id
        {
            return Err(TouchSessionConflict::SessionBoundElsewhere {
                owner_project: session.project_key.clone(),
                owner_initiative: session.initiative_id,
            });
        }
    }
    Ok(TouchSession {
        session_id: input.session_id,
        project_key: input.project_key,
        initiative_id: input.initiative_id,
        now: input.now,
    })
}

/// Read a session's state back out of its stored parts.
///
/// Kept here so an adapter has one place to call rather than reimplementing the
/// mapping between `status`, `current_ticket_id`, and [`SessionState`].
pub fn session_state(status: &str, current_ticket_id: Option<i64>) -> Result<SessionState> {
    SessionState::from_persisted(crate::model::PersistedSessionState {
        status,
        current_ticket_id,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{active_sessions, prepare_touch_session, session_of, SessionBudget, TouchInput};
    use crate::id::{InitiativeId, ProjectKey, SessionId, TicketId};
    use crate::initiative::InitiativeView;
    use crate::model::{
        ActiveSessionState, Initiative, PersistedInitiativeStatus, Session, SessionState,
        TicketType,
    };
    use crate::outcome::TouchSessionConflict;
    use crate::storage::InitiativeRevision;
    use crate::time::Timestamp;

    fn key() -> ProjectKey {
        ProjectKey::new("/Users/example/project").unwrap()
    }

    fn moment(text: &str) -> Timestamp {
        Timestamp::from_str(text).unwrap()
    }

    fn session(name: &str, state: SessionState, seen: &str, count: u32) -> Session {
        Session {
            id: SessionId::new(name).unwrap(),
            project_key: key(),
            initiative_id: Some(InitiativeId::new(1).unwrap()),
            state,
            resolved_non_research_count: count,
            started_at: moment("2026-08-02 13:00:00"),
            last_seen_at: moment(seen),
        }
    }

    fn view(sessions: Vec<Session>) -> InitiativeView {
        InitiativeView::new(
            Initiative {
                id: InitiativeId::new(1).unwrap(),
                project_key: key(),
                name: "Cache the map".to_string(),
                destination: "A map that loads instantly".to_string(),
                notes: String::new(),
                status: PersistedInitiativeStatus::Working,
                created_at: moment("2026-08-02 13:00:00"),
            },
            Vec::new(),
            Vec::new(),
            sessions,
            InitiativeRevision::INITIAL,
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_session_holds_nothing_and_has_spent_nothing() {
        let budget = SessionBudget::of_optional(None);
        assert_eq!(budget, SessionBudget::FRESH);
        assert!(budget.holding().is_none());
        assert!(!budget.non_research_spent());
        assert!(budget.may_take(TicketType::Task));
    }

    #[test]
    fn a_holding_session_reports_the_ticket_it_holds() {
        let held = TicketId::new(4).unwrap();
        let record = session(
            "session-1",
            SessionState::Active(ActiveSessionState::Holding { ticket_id: held }),
            "2026-08-02 13:45:09",
            0,
        );
        assert_eq!(SessionBudget::of(&record).holding(), Some(held));
    }

    #[test]
    fn the_non_research_limit_is_permanent_and_never_blocks_research() {
        let spent = session(
            "session-1",
            SessionState::Active(ActiveSessionState::Ready),
            "2026-08-02 13:45:09",
            1,
        );
        let budget = SessionBudget::of(&spent);
        assert!(budget.non_research_spent());
        assert!(budget.may_take(TicketType::Research));
        assert!(!budget.may_take(TicketType::Task));
        assert!(!budget.may_take(TicketType::Grilling));
        assert!(!budget.may_take(TicketType::Prototype));
    }

    #[test]
    fn active_sessions_are_listed_by_newest_activity_then_by_identifier() {
        let view = view(vec![
            session(
                "session-b",
                SessionState::Active(ActiveSessionState::Ready),
                "2026-08-02 13:45:09",
                0,
            ),
            session(
                "session-a",
                SessionState::Active(ActiveSessionState::Ready),
                "2026-08-02 13:45:09",
                0,
            ),
            session(
                "session-c",
                SessionState::Active(ActiveSessionState::Ready),
                "2026-08-02 14:00:00",
                0,
            ),
            session("session-d", SessionState::Closed, "2026-08-02 15:00:00", 0),
        ]);
        let names: Vec<&str> = active_sessions(&view)
            .iter()
            .map(|session| session.id.as_str())
            .collect();
        assert_eq!(names, vec!["session-c", "session-a", "session-b"]);
    }

    #[test]
    fn a_session_is_found_by_its_identifier() {
        let view = view(vec![session(
            "session-1",
            SessionState::Active(ActiveSessionState::Ready),
            "2026-08-02 13:45:09",
            0,
        )]);
        assert!(session_of(&view, &SessionId::new("session-1").unwrap()).is_some());
        assert!(session_of(&view, &SessionId::new("session-2").unwrap()).is_none());
    }

    #[test]
    fn a_new_session_binds_to_wherever_it_first_appeared() {
        let input = TouchInput {
            session_id: SessionId::new("session-1").unwrap(),
            project_key: key(),
            initiative_id: Some(InitiativeId::new(1).unwrap()),
            now: moment("2026-08-02 13:45:09"),
        };
        let command = prepare_touch_session(input.clone(), None).unwrap();
        assert_eq!(command.session_id, input.session_id);
        assert_eq!(command.initiative_id, input.initiative_id);
    }

    #[test]
    fn a_session_never_moves_to_another_project_or_initiative() {
        let existing = session(
            "session-1",
            SessionState::Active(ActiveSessionState::Ready),
            "2026-08-02 13:45:09",
            0,
        );
        let elsewhere = TouchInput {
            session_id: SessionId::new("session-1").unwrap(),
            project_key: ProjectKey::new("/Users/example/other").unwrap(),
            initiative_id: Some(InitiativeId::new(1).unwrap()),
            now: moment("2026-08-02 13:45:09"),
        };
        assert_eq!(
            prepare_touch_session(elsewhere, Some(&existing)),
            Err(TouchSessionConflict::SessionBoundElsewhere {
                owner_project: key(),
                owner_initiative: Some(InitiativeId::new(1).unwrap()),
            })
        );

        let other_initiative = TouchInput {
            session_id: SessionId::new("session-1").unwrap(),
            project_key: key(),
            initiative_id: Some(InitiativeId::new(2).unwrap()),
            now: moment("2026-08-02 13:45:09"),
        };
        assert!(prepare_touch_session(other_initiative, Some(&existing)).is_err());
    }
}
