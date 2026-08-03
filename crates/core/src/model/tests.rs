#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{
    ActiveSessionState, Dependency, InitiativeState, NonEmptyVec, PersistedClaim,
    PersistedInitiativeStatus, PersistedSessionState, PersistedTicketState, SessionState,
    TicketState, TicketType,
};
use crate::error::Error;
use crate::id::{SessionId, TicketId};

fn ticket_id(value: i64) -> TicketId {
    TicketId::new(value).unwrap()
}

#[test]
fn ticket_types_round_trip_through_their_stored_text() {
    for kind in TicketType::ALL {
        assert_eq!(kind.as_str().parse::<TicketType>().unwrap(), kind);
    }
    assert_eq!(TicketType::Research.to_string(), "research");
}

#[test]
fn ticket_types_reject_anything_outside_the_check_constraint() {
    assert!("bug".parse::<TicketType>().is_err());
    assert!("Research".parse::<TicketType>().is_err());
    assert!("".parse::<TicketType>().is_err());
}

#[test]
fn only_research_tickets_are_research() {
    assert!(TicketType::Research.is_research());
    assert!(!TicketType::Task.is_research());
    assert!(!TicketType::Grilling.is_research());
    assert!(!TicketType::Prototype.is_research());
}

#[test]
fn initiative_statuses_round_trip_through_their_stored_text() {
    for status in [
        PersistedInitiativeStatus::Charting,
        PersistedInitiativeStatus::Working,
        PersistedInitiativeStatus::Clear,
    ] {
        assert_eq!(
            status
                .as_str()
                .parse::<PersistedInitiativeStatus>()
                .unwrap(),
            status
        );
    }
    assert!("cleared".parse::<PersistedInitiativeStatus>().is_err());
    assert!(PersistedInitiativeStatus::Clear.is_clear());
    assert!(!PersistedInitiativeStatus::Working.is_clear());
}

#[test]
fn non_empty_vec_rejects_an_empty_vector() {
    assert!(matches!(
        NonEmptyVec::<u8>::try_from(Vec::new()),
        Err(Error::InvalidValue { .. })
    ));
}

#[test]
fn non_empty_vec_exposes_a_first_element_and_a_non_zero_count() {
    let values = NonEmptyVec::try_from(vec![3, 1, 2]).unwrap();
    assert_eq!(*values.first(), 3);
    assert_eq!(values.count().get(), 3);
    assert_eq!(values.as_slice(), &[3, 1, 2]);
    assert_eq!(values.into_vec(), vec![3, 1, 2]);
}

#[test]
fn an_open_ticket_parses_from_a_bare_row() {
    let state = TicketState::from_persisted(PersistedTicketState {
        status: "open",
        ..PersistedTicketState::default()
    })
    .unwrap();
    assert_eq!(state, TicketState::Open);
    assert!(state.is_unresolved());
    assert!(state.blocks_dependents());
}

#[test]
fn a_claimed_ticket_carries_its_claimant() {
    let state = TicketState::from_persisted(PersistedTicketState {
        status: "claimed",
        live_claim: Some(PersistedClaim {
            session_id: "session-1",
            claimed_at: "2026-08-02 10:00:00",
        }),
        ..PersistedTicketState::default()
    })
    .unwrap();
    assert_eq!(
        state.claimant(),
        Some(&SessionId::new("session-1").unwrap())
    );
    assert!(state.is_unresolved());
}

#[test]
fn a_resolved_ticket_carries_its_decision() {
    let state = TicketState::from_persisted(PersistedTicketState {
        status: "resolved",
        resolution: Some("We use SQLite."),
        resolved_at: Some("2026-08-02 11:00:00"),
        ..PersistedTicketState::default()
    })
    .unwrap();
    assert_eq!(state.resolution(), Some("We use SQLite."));
    assert!(!state.is_unresolved());
    assert!(!state.blocks_dependents());
}

#[test]
fn a_resolved_ticket_may_record_an_amendment() {
    let state = TicketState::from_persisted(PersistedTicketState {
        status: "resolved",
        resolution: Some("We use SQLite."),
        resolved_at: Some("2026-08-02 11:00:00"),
        amended_at: Some("2026-08-02 12:00:00"),
        ..PersistedTicketState::default()
    })
    .unwrap();
    assert!(matches!(
        state,
        TicketState::Resolved {
            amended_at: Some(_),
            ..
        }
    ));
}

#[test]
fn a_claimed_ticket_without_a_live_claim_is_corrupt() {
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "claimed",
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
}

#[test]
fn an_open_ticket_with_a_live_claim_is_corrupt() {
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "open",
            live_claim: Some(PersistedClaim {
                session_id: "session-1",
                claimed_at: "2026-08-02 10:00:00",
            }),
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
}

#[test]
fn a_resolved_ticket_without_a_resolution_is_corrupt() {
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "resolved",
            resolved_at: Some("2026-08-02 11:00:00"),
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "resolved",
            resolution: Some(""),
            resolved_at: Some("2026-08-02 11:00:00"),
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
}

#[test]
fn a_resolved_ticket_without_a_resolved_at_is_corrupt() {
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "resolved",
            resolution: Some("We use SQLite."),
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
}

#[test]
fn an_open_ticket_that_carries_decision_columns_is_corrupt() {
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "open",
            resolution: Some("leaked"),
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "open",
            amended_at: Some("2026-08-02 12:00:00"),
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
}

#[test]
fn an_unknown_ticket_status_is_corrupt() {
    assert!(matches!(
        TicketState::from_persisted(PersistedTicketState {
            status: "in-progress",
            ..PersistedTicketState::default()
        }),
        Err(Error::CorruptData {
            entity: "ticket",
            ..
        })
    ));
}

#[test]
fn an_excluded_ticket_parses_and_still_blocks_dependents() {
    let state = TicketState::from_persisted(PersistedTicketState {
        status: "excluded",
        ..PersistedTicketState::default()
    })
    .unwrap();
    assert_eq!(state, TicketState::Excluded);
    assert!(!state.is_unresolved());
    assert!(state.blocks_dependents());
}

#[test]
fn ticket_states_report_their_stored_status_text() {
    assert_eq!(TicketState::Open.as_status_str(), "open");
    assert_eq!(TicketState::Excluded.as_status_str(), "excluded");
}

#[test]
fn an_active_session_is_ready_or_holding() {
    assert_eq!(
        SessionState::from_persisted(PersistedSessionState {
            status: "active",
            current_ticket_id: None,
        })
        .unwrap(),
        SessionState::Active(ActiveSessionState::Ready)
    );
    assert_eq!(
        SessionState::from_persisted(PersistedSessionState {
            status: "active",
            current_ticket_id: Some(4),
        })
        .unwrap()
        .held_ticket(),
        Some(ticket_id(4))
    );
}

#[test]
fn a_closed_session_that_still_holds_a_ticket_is_corrupt() {
    assert!(matches!(
        SessionState::from_persisted(PersistedSessionState {
            status: "closed",
            current_ticket_id: Some(4),
        }),
        Err(Error::CorruptData {
            entity: "session",
            ..
        })
    ));
}

#[test]
fn an_unknown_session_status_is_corrupt() {
    assert!(matches!(
        SessionState::from_persisted(PersistedSessionState {
            status: "paused",
            current_ticket_id: None,
        }),
        Err(Error::CorruptData {
            entity: "session",
            ..
        })
    ));
}

#[test]
fn a_session_holding_a_non_positive_ticket_id_is_corrupt() {
    assert!(matches!(
        SessionState::from_persisted(PersistedSessionState {
            status: "active",
            current_ticket_id: Some(0),
        }),
        Err(Error::CorruptData { .. })
    ));
}

#[test]
fn dependencies_reject_a_self_edge() {
    assert!(Dependency::new(ticket_id(1), ticket_id(2)).is_ok());
    assert!(matches!(
        Dependency::new(ticket_id(1), ticket_id(1)),
        Err(Error::InvalidValue {
            field: "dependency",
            ..
        })
    ));
}

#[test]
fn dependency_accessors_keep_the_direction_straight() {
    let edge = Dependency::new(ticket_id(5), ticket_id(3)).unwrap();
    assert_eq!(edge.ticket_id(), ticket_id(5));
    assert_eq!(edge.blocker_id(), ticket_id(3));
}

#[test]
fn initiative_states_report_one_word_each() {
    assert_eq!(InitiativeState::Charting.as_str(), "charting");
    assert_eq!(InitiativeState::Complete.as_str(), "complete");
    assert_eq!(InitiativeState::Clear.to_string(), "clear");
}
