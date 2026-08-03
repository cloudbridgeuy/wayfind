//! What the SQLite adapter promises, exercised through the traits alone.
//!
//! Every test here drives a database made by `SqliteStorage::initialize` in a
//! temporary directory. Nothing reads the operator's own database, and nothing
//! reaches around the trait boundary to set up state that a command could set up
//! instead — a test that writes its own rows proves the test's SQL works, not the
//! adapter's.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use tempfile::TempDir;
use wayfind_cli::sqlite::SqliteStorage;
use wayfind_core::{
    AddAttachmentReference, AddFogNote, AddScopeExclusion, AllocatedId, AmendConflict,
    AmendOutcome, AmendTicket, AtomicWorkflows, AttachmentName, AttachmentStore, ClaimConflict,
    ClaimOutcome, ClaimTicket, ClearConflict, ClearInitiative, ClearOutcome, Consistency,
    CreateInitiative, CreateTicket, EnsureProject, EntityReader, EntityWriter, IdAllocator,
    IdScope, InitiativeId, InitiativeRevision, InsertDependency, InsertDependencyConflict,
    InsertDependencyOutcome, NonEmptyText, ProjectKey, ReferenceConflict, ReferenceOutcome,
    RemoveAttachmentOutcome, RemoveAttachmentReference, ResolutionText, ResolveConflict,
    ResolveOutcome, ResolveTicket, SessionId, TicketId, TicketState, TicketType, Timestamp,
    TouchSession, TouchSessionConflict, TouchSessionOutcome,
};

const PROJECT: &str = "/Users/operator/Projects/wayfind";

/// A fresh database in a temporary directory, and the directory that holds it.
///
/// The directory is returned so the caller keeps it alive; dropping it deletes
/// the database.
fn store() -> (TempDir, SqliteStorage) {
    let directory = TempDir::new().expect("make a temporary directory");
    let path: PathBuf = directory.path().join("wayfind.sqlite");
    let storage = SqliteStorage::initialize(&path).expect("create the database");
    (directory, storage)
}

fn at(text: &str) -> Timestamp {
    text.parse().expect("parse a timestamp")
}

fn now() -> Timestamp {
    at("2026-08-02 09:00:00")
}

fn project_key() -> ProjectKey {
    ProjectKey::new(PROJECT).expect("parse the project key")
}

fn session(name: &str) -> SessionId {
    SessionId::new(name).expect("parse the session id")
}

fn text(value: &str) -> NonEmptyText {
    NonEmptyText::new(value).expect("parse the text")
}

/// A project and one initiative, ready for tickets.
fn initiative(storage: &SqliteStorage) -> InitiativeId {
    storage
        .ensure_project(EnsureProject {
            key: project_key(),
            now: now(),
        })
        .expect("record the project");
    let id = match storage.allocate(IdScope::Initiative).expect("allocate") {
        AllocatedId::Initiative(id) => id,
        other => panic!("expected an initiative id, got {other:?}"),
    };
    storage
        .create_initiative(CreateInitiative {
            id,
            project_key: project_key(),
            name: text("Port the tracker to Rust"),
            destination: text("One binary, the same database"),
            notes: String::new(),
            now: now(),
        })
        .expect("create the initiative");
    id
}

/// One ticket of the given kind, at a freshly allocated identifier.
fn ticket(
    storage: &SqliteStorage,
    initiative_id: InitiativeId,
    title: &str,
    ticket_type: TicketType,
) -> TicketId {
    let id = match storage.allocate(IdScope::Ticket).expect("allocate") {
        AllocatedId::Ticket(id) => id,
        other => panic!("expected a ticket id, got {other:?}"),
    };
    storage
        .create_ticket(CreateTicket {
            id,
            initiative_id,
            title: text(title),
            ticket_type,
            question: format!("What about {title}?"),
            now: now(),
        })
        .expect("create the ticket");
    id
}

/// The session recorded against the initiative, so a claim has somewhere to go.
fn start(storage: &SqliteStorage, initiative_id: InitiativeId, name: &str) -> SessionId {
    let id = session(name);
    let outcome = storage
        .touch_session(TouchSession {
            session_id: id.clone(),
            project_key: project_key(),
            initiative_id: Some(initiative_id),
            now: now(),
        })
        .expect("record the session");
    assert!(matches!(outcome, TouchSessionOutcome::Started(_)));
    id
}

fn revision(storage: &SqliteStorage, initiative_id: InitiativeId) -> InitiativeRevision {
    storage
        .initiative_revision(initiative_id, Consistency::Strong)
        .expect("read the revision")
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

#[test]
fn identifiers_move_forward_and_never_repeat() {
    let (_directory, storage) = store();
    let first = storage.allocate(IdScope::Ticket).expect("allocate");
    let second = storage.allocate(IdScope::Ticket).expect("allocate");
    assert_eq!(first, AllocatedId::Ticket(TicketId::new(1).unwrap()));
    assert_eq!(second, AllocatedId::Ticket(TicketId::new(2).unwrap()));
}

#[test]
fn identifiers_start_above_rows_the_script_wrote() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    // The initiative above was written at identifier 1 through the counter. A
    // row written without one — as every row in a script database was — must
    // still not be overwritten.
    storage
        .connection()
        .execute(
            "INSERT INTO tickets(id, initiative_id, title, type, status, question) \
             VALUES (40, ?1, 'From the script', 'task', 'open', 'why');",
            [initiative_id.get()],
        )
        .expect("write a ticket the way the script did");

    let allocated = storage.allocate(IdScope::Ticket).expect("allocate");
    assert_eq!(allocated, AllocatedId::Ticket(TicketId::new(41).unwrap()));
}

#[test]
fn each_scope_counts_on_its_own() {
    let (_directory, storage) = store();
    let ticket_id = storage.allocate(IdScope::Ticket).expect("allocate");
    let note_id = storage.allocate(IdScope::FogNote).expect("allocate");
    let exclusion_id = storage.allocate(IdScope::ScopeExclusion).expect("allocate");
    assert_eq!(ticket_id, AllocatedId::Ticket(TicketId::new(1).unwrap()));
    // Fog notes and exclusions share a wrapper type but not a table, so both
    // start at one.
    assert_eq!(note_id, exclusion_id);
}

#[test]
fn allocating_does_not_move_an_initiative_on() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let before = revision(&storage, initiative_id);
    storage.allocate(IdScope::Ticket).expect("allocate");
    assert_eq!(revision(&storage, initiative_id), before);
}

// ---------------------------------------------------------------------------
// Simple writes
// ---------------------------------------------------------------------------

#[test]
fn recording_a_project_twice_keeps_the_first_time() {
    let (_directory, storage) = store();
    let first = storage
        .ensure_project(EnsureProject {
            key: project_key(),
            now: at("2026-08-01 08:00:00"),
        })
        .expect("record the project");
    let second = storage
        .ensure_project(EnsureProject {
            key: project_key(),
            now: at("2026-08-02 08:00:00"),
        })
        .expect("record the project again");
    assert_eq!(first, second);
}

#[test]
fn a_new_initiative_starts_charting_and_is_the_newest() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let stored = storage
        .initiative(initiative_id, Consistency::Strong)
        .expect("read the initiative")
        .expect("the initiative is there");
    assert_eq!(stored.status.as_str(), "charting");
    assert_eq!(stored.name, "Port the tracker to Rust");
}

#[test]
fn a_new_ticket_is_open_and_moves_its_initiative_on() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let before = revision(&storage, initiative_id);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let stored = storage
        .ticket(ticket_id, Consistency::Strong)
        .expect("read the ticket")
        .expect("the ticket is there");
    assert_eq!(stored.state, TicketState::Open);
    assert_eq!(revision(&storage, initiative_id), before.next());
}

#[test]
fn a_session_is_started_once_and_refreshed_after_that() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let id = start(&storage, initiative_id, "session-one");
    let again = storage
        .touch_session(TouchSession {
            session_id: id,
            project_key: project_key(),
            initiative_id: Some(initiative_id),
            now: at("2026-08-02 10:00:00"),
        })
        .expect("record the session again");
    let TouchSessionOutcome::Refreshed(refreshed) = again else {
        panic!("expected a refresh, got {again:?}");
    };
    assert_eq!(refreshed.last_seen_at, at("2026-08-02 10:00:00"));
    assert_eq!(refreshed.started_at, now());
}

#[test]
fn a_session_cannot_be_reused_in_another_initiative() {
    let (_directory, storage) = store();
    let first = initiative(&storage);
    let id = start(&storage, first, "session-one");

    let second = match storage.allocate(IdScope::Initiative).expect("allocate") {
        AllocatedId::Initiative(id) => id,
        other => panic!("expected an initiative id, got {other:?}"),
    };
    storage
        .create_initiative(CreateInitiative {
            id: second,
            project_key: project_key(),
            name: text("Something else entirely"),
            destination: text("Elsewhere"),
            notes: String::new(),
            now: now(),
        })
        .expect("create the second initiative");

    let outcome = storage
        .touch_session(TouchSession {
            session_id: id,
            project_key: project_key(),
            initiative_id: Some(second),
            now: now(),
        })
        .expect("record the session");
    assert_eq!(
        outcome,
        TouchSessionOutcome::Conflict(TouchSessionConflict::SessionBoundElsewhere {
            owner_project: project_key(),
            owner_initiative: Some(first),
        })
    );
}

#[test]
fn a_heartbeat_does_not_move_the_initiative_on() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let id = start(&storage, initiative_id, "session-one");
    let before = revision(&storage, initiative_id);
    storage
        .touch_session(TouchSession {
            session_id: id,
            project_key: project_key(),
            initiative_id: Some(initiative_id),
            now: at("2026-08-02 11:00:00"),
        })
        .expect("record the session");
    assert_eq!(revision(&storage, initiative_id), before);
}

#[test]
fn fog_and_exclusions_are_recorded_against_their_initiative() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let AllocatedId::Note(fog_id) = storage.allocate(IdScope::FogNote).expect("allocate") else {
        panic!("expected a note id");
    };
    storage
        .add_fog_note(AddFogNote {
            id: fog_id,
            initiative_id,
            note: text("Nobody has said which store wins"),
            now: now(),
        })
        .expect("add the fog note");
    let AllocatedId::Note(exclusion_id) =
        storage.allocate(IdScope::ScopeExclusion).expect("allocate")
    else {
        panic!("expected a note id");
    };
    storage
        .add_scope_exclusion(AddScopeExclusion {
            id: exclusion_id,
            initiative_id,
            note: text("No web interface"),
            now: now(),
        })
        .expect("add the exclusion");

    let fog = storage
        .fog_notes(initiative_id, Consistency::Strong)
        .expect("read the fog");
    let exclusions = storage
        .scope_exclusions(initiative_id, Consistency::Strong)
        .expect("read the exclusions");
    assert_eq!(fog.len(), 1);
    assert_eq!(exclusions.len(), 1);
    assert_eq!(fog[0].note, "Nobody has said which store wins");
    assert_eq!(exclusions[0].note, "No web interface");
}

// ---------------------------------------------------------------------------
// Claiming
// ---------------------------------------------------------------------------

/// Claim a ticket at whatever revision the initiative currently sits at.
fn claim(
    storage: &SqliteStorage,
    initiative_id: InitiativeId,
    ticket_id: TicketId,
    session_id: &SessionId,
) -> ClaimOutcome {
    storage
        .claim_ticket(ClaimTicket {
            ticket_id,
            initiative_id,
            expected_initiative_revision: revision(storage, initiative_id),
            session_id: session_id.clone(),
            expected_session_holds: None,
            expected_claimant: None,
            now: now(),
        })
        .expect("claim the ticket")
}

#[test]
fn claiming_takes_the_ticket_and_binds_the_session() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");

    assert_eq!(
        claim(&storage, initiative_id, ticket_id, &session_id),
        ClaimOutcome::Claimed
    );
    let stored = storage
        .ticket(ticket_id, Consistency::Strong)
        .expect("read the ticket")
        .expect("the ticket is there");
    assert_eq!(stored.state.claimant(), Some(&session_id));
    let held = storage
        .session(&session_id, Consistency::Strong)
        .expect("read the session")
        .expect("the session is there");
    assert_eq!(held.state.held_ticket(), Some(ticket_id));
}

#[test]
fn claiming_the_same_ticket_twice_changes_nothing() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");

    claim(&storage, initiative_id, ticket_id, &session_id);
    let before = revision(&storage, initiative_id);
    assert_eq!(
        claim(&storage, initiative_id, ticket_id, &session_id),
        ClaimOutcome::AlreadyHeld
    );
    assert_eq!(revision(&storage, initiative_id), before);
}

#[test]
fn a_ticket_another_session_holds_is_refused() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let first = start(&storage, initiative_id, "session-one");
    let second = start(&storage, initiative_id, "session-two");

    claim(&storage, initiative_id, ticket_id, &first);
    assert_eq!(
        claim(&storage, initiative_id, ticket_id, &second),
        ClaimOutcome::Conflict(ClaimConflict::AlreadyClaimed {
            ticket_id,
            claimant: first,
        })
    );
}

#[test]
fn a_session_already_holding_something_cannot_take_more() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let held = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let other = ticket(&storage, initiative_id, "Choose a format", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");

    claim(&storage, initiative_id, held, &session_id);
    assert_eq!(
        claim(&storage, initiative_id, other, &session_id),
        ClaimOutcome::Conflict(ClaimConflict::SessionHoldsAnotherTicket { held })
    );
}

#[test]
fn a_claim_against_an_old_revision_is_refused() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");
    let stale = revision(&storage, initiative_id);
    // Somebody else adds a ticket between the read and the claim.
    ticket(&storage, initiative_id, "Choose a format", TicketType::Task);

    let outcome = storage
        .claim_ticket(ClaimTicket {
            ticket_id,
            initiative_id,
            expected_initiative_revision: stale,
            session_id,
            expected_session_holds: None,
            expected_claimant: None,
            now: now(),
        })
        .expect("claim the ticket");
    let ClaimOutcome::Conflict(ClaimConflict::StaleRevision(conflict)) = outcome else {
        panic!("expected a stale revision, got {outcome:?}");
    };
    assert_eq!(conflict.expected, stale);
    assert_eq!(conflict.actual, stale.next());
}

#[test]
fn a_ticket_outside_the_initiative_is_refused() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let session_id = start(&storage, initiative_id, "session-one");
    let missing = TicketId::new(404).unwrap();

    assert_eq!(
        claim(&storage, initiative_id, missing, &session_id),
        ClaimOutcome::Conflict(ClaimConflict::NoSuchTicket { ticket_id: missing })
    );
}

// ---------------------------------------------------------------------------
// Resolving
// ---------------------------------------------------------------------------

/// Which ticket of which initiative a resolution settles.
#[derive(Debug, Clone, Copy)]
struct Settle {
    initiative_id: InitiativeId,
    ticket_id: TicketId,
}

/// Resolve a ticket at whatever revision the initiative currently sits at.
fn resolve(
    storage: &SqliteStorage,
    settle: Settle,
    session_id: &SessionId,
    ticket_type: TicketType,
    resolution: &str,
) -> ResolveOutcome {
    let Settle {
        initiative_id,
        ticket_id,
    } = settle;

    let AllocatedId::Decision(decision_id) = storage.allocate(IdScope::Decision).expect("allocate")
    else {
        panic!("expected a decision id");
    };
    storage
        .resolve_ticket(ResolveTicket {
            ticket_id,
            initiative_id,
            expected_initiative_revision: revision(storage, initiative_id),
            session_id: session_id.clone(),
            decision_id,
            ticket_type,
            resolution: ResolutionText::new(resolution).expect("parse the resolution"),
            now: at("2026-08-02 12:00:00"),
        })
        .expect("resolve the ticket")
}

#[test]
fn resolving_settles_the_ticket_frees_the_session_and_records_a_decision() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");
    claim(&storage, initiative_id, ticket_id, &session_id);

    assert_eq!(
        resolve(
            &storage,
            Settle {
                initiative_id,
                ticket_id
            },
            &session_id,
            TicketType::Task,
            "SQLite stays.\nIt is one file and the script already used it.",
        ),
        ResolveOutcome::Resolved
    );

    let stored = storage
        .ticket(ticket_id, Consistency::Strong)
        .expect("read the ticket")
        .expect("the ticket is there");
    let TicketState::Resolved {
        resolution,
        resolved_at,
        amended_at,
    } = stored.state
    else {
        panic!("expected a resolved ticket, got {:?}", stored.state);
    };
    assert!(resolution.starts_with("SQLite stays."));
    assert_eq!(resolved_at, at("2026-08-02 12:00:00"));
    assert_eq!(amended_at, None);

    let freed = storage
        .session(&session_id, Consistency::Strong)
        .expect("read the session")
        .expect("the session is there");
    assert_eq!(freed.state.held_ticket(), None);
    assert_eq!(freed.resolved_non_research_count, 1);

    let decisions = storage
        .decisions(initiative_id, Consistency::Strong)
        .expect("read the decisions");
    assert_eq!(decisions.len(), 1);
    // The gist is the first line, exactly as the script split it.
    assert_eq!(decisions[0].gist, "SQLite stays.");
}

#[test]
fn research_costs_a_session_nothing() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let first = ticket(
        &storage,
        initiative_id,
        "Read the docs",
        TicketType::Research,
    );
    let second = ticket(
        &storage,
        initiative_id,
        "Read more docs",
        TicketType::Research,
    );
    let session_id = start(&storage, initiative_id, "session-one");

    claim(&storage, initiative_id, first, &session_id);
    resolve(
        &storage,
        Settle {
            initiative_id,
            ticket_id: first,
        },
        &session_id,
        TicketType::Research,
        "Rusqlite bundles SQLite.",
    );
    claim(&storage, initiative_id, second, &session_id);
    assert_eq!(
        resolve(
            &storage,
            Settle {
                initiative_id,
                ticket_id: second
            },
            &session_id,
            TicketType::Research,
            "FTS5 is compiled in.",
        ),
        ResolveOutcome::Resolved
    );
    let spent = storage
        .session(&session_id, Consistency::Strong)
        .expect("read the session")
        .expect("the session is there");
    assert_eq!(spent.resolved_non_research_count, 0);
}

#[test]
fn a_session_gets_one_non_research_resolution_and_no_more() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let first = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let second = ticket(&storage, initiative_id, "Choose a format", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");

    claim(&storage, initiative_id, first, &session_id);
    resolve(
        &storage,
        Settle {
            initiative_id,
            ticket_id: first,
        },
        &session_id,
        TicketType::Task,
        "SQLite stays.",
    );

    // The budget is checked at the claim, before any work is done, rather than
    // at the resolution after it.
    assert_eq!(
        claim(&storage, initiative_id, second, &session_id),
        ClaimOutcome::Conflict(ClaimConflict::NonResearchBudgetSpent { ticket_id: second })
    );
}

#[test]
fn only_the_session_holding_a_ticket_may_settle_it() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let holder = start(&storage, initiative_id, "session-one");
    let stranger = start(&storage, initiative_id, "session-two");
    claim(&storage, initiative_id, ticket_id, &holder);

    assert_eq!(
        resolve(
            &storage,
            Settle {
                initiative_id,
                ticket_id
            },
            &stranger,
            TicketType::Task,
            "Mine now.",
        ),
        ResolveOutcome::Conflict(ResolveConflict::ClaimedByAnotherSession {
            ticket_id,
            claimant: holder,
        })
    );
}

#[test]
fn an_unclaimed_ticket_cannot_be_settled() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);
    let session_id = start(&storage, initiative_id, "session-one");

    assert_eq!(
        resolve(
            &storage,
            Settle {
                initiative_id,
                ticket_id
            },
            &session_id,
            TicketType::Task,
            "Skipping the queue.",
        ),
        ResolveOutcome::Conflict(ResolveConflict::NotClaimed { ticket_id })
    );
}

#[test]
fn a_settled_ticket_stays_settled() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let session_id = start(&storage, initiative_id, "session-one");
    claim(&storage, initiative_id, ticket_id, &session_id);
    resolve(
        &storage,
        Settle {
            initiative_id,
            ticket_id,
        },
        &session_id,
        TicketType::Research,
        "SQLite stays.",
    );

    assert_eq!(
        resolve(
            &storage,
            Settle {
                initiative_id,
                ticket_id
            },
            &session_id,
            TicketType::Research,
            "Actually, no.",
        ),
        ResolveOutcome::Conflict(ResolveConflict::AlreadyResolved { ticket_id })
    );
}

// ---------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------

fn depend(
    storage: &SqliteStorage,
    initiative_id: InitiativeId,
    ticket_id: TicketId,
    blocker_id: TicketId,
) -> InsertDependencyOutcome {
    storage
        .insert_dependency(InsertDependency {
            ticket_id,
            blocker_id,
            initiative_id,
            expected_initiative_revision: revision(storage, initiative_id),
            now: now(),
        })
        .expect("insert the dependency")
}

#[test]
fn an_edge_is_added_once_and_asking_twice_is_harmless() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let waiting = ticket(&storage, initiative_id, "Ship it", TicketType::Task);
    let blocker = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);

    assert_eq!(
        depend(&storage, initiative_id, waiting, blocker),
        InsertDependencyOutcome::Inserted
    );
    assert_eq!(
        depend(&storage, initiative_id, waiting, blocker),
        InsertDependencyOutcome::AlreadyPresent
    );
    let edges = storage
        .dependencies(initiative_id, Consistency::Strong)
        .expect("read the edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].ticket_id(), waiting);
    assert_eq!(edges[0].blocker_id(), blocker);
}

#[test]
fn a_ticket_cannot_wait_on_itself() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Ship it", TicketType::Task);

    assert_eq!(
        depend(&storage, initiative_id, ticket_id, ticket_id),
        InsertDependencyOutcome::Conflict(InsertDependencyConflict::SelfEdge { ticket_id })
    );
}

#[test]
fn an_edge_to_a_missing_ticket_is_refused() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let waiting = ticket(&storage, initiative_id, "Ship it", TicketType::Task);
    let missing = TicketId::new(404).unwrap();

    assert_eq!(
        depend(&storage, initiative_id, waiting, missing),
        InsertDependencyOutcome::Conflict(InsertDependencyConflict::NoSuchTicket {
            ticket_id: missing
        })
    );
}

// ---------------------------------------------------------------------------
// Amending and clearing
// ---------------------------------------------------------------------------

#[test]
fn amending_repairs_both_the_recorded_text_and_its_gist() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let session_id = start(&storage, initiative_id, "session-one");
    claim(&storage, initiative_id, ticket_id, &session_id);
    resolve(
        &storage,
        Settle {
            initiative_id,
            ticket_id,
        },
        &session_id,
        TicketType::Research,
        "SQLightning stays.",
    );

    let outcome = storage
        .amend_ticket(AmendTicket {
            ticket_id,
            initiative_id,
            expected_initiative_revision: revision(&storage, initiative_id),
            resolution: ResolutionText::new("SQLite stays.\nThe name was mistyped.")
                .expect("parse the resolution"),
            now: at("2026-08-02 13:00:00"),
        })
        .expect("amend the ticket");
    assert_eq!(outcome, AmendOutcome::Amended);

    let stored = storage
        .ticket(ticket_id, Consistency::Strong)
        .expect("read the ticket")
        .expect("the ticket is there");
    let TicketState::Resolved {
        resolution,
        amended_at,
        ..
    } = stored.state
    else {
        panic!("expected a resolved ticket");
    };
    assert!(resolution.starts_with("SQLite stays."));
    assert_eq!(amended_at, Some(at("2026-08-02 13:00:00")));

    let decisions = storage
        .decisions(initiative_id, Consistency::Strong)
        .expect("read the decisions");
    assert_eq!(decisions[0].gist, "SQLite stays.");
}

#[test]
fn an_unresolved_ticket_cannot_be_amended() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(&storage, initiative_id, "Choose a store", TicketType::Task);

    let outcome = storage
        .amend_ticket(AmendTicket {
            ticket_id,
            initiative_id,
            expected_initiative_revision: revision(&storage, initiative_id),
            resolution: ResolutionText::new("Nothing to repair.").expect("parse the resolution"),
            now: now(),
        })
        .expect("amend the ticket");
    let AmendOutcome::Conflict(AmendConflict::NotResolved { status, .. }) = outcome else {
        panic!("expected a not-resolved conflict, got {outcome:?}");
    };
    assert_eq!(status.as_str(), "open");
}

#[test]
fn an_initiative_closes_only_once_no_work_is_outstanding() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let session_id = start(&storage, initiative_id, "session-one");

    let refused = storage
        .clear_initiative(ClearInitiative {
            initiative_id,
            expected_initiative_revision: revision(&storage, initiative_id),
            now: now(),
        })
        .expect("clear the initiative");
    assert_eq!(
        refused,
        ClearOutcome::Conflict(ClearConflict::OpenTicketsRemain { outstanding: 1 })
    );

    claim(&storage, initiative_id, ticket_id, &session_id);
    resolve(
        &storage,
        Settle {
            initiative_id,
            ticket_id,
        },
        &session_id,
        TicketType::Research,
        "SQLite stays.",
    );

    assert_eq!(
        storage
            .clear_initiative(ClearInitiative {
                initiative_id,
                expected_initiative_revision: revision(&storage, initiative_id),
                now: now(),
            })
            .expect("clear the initiative"),
        ClearOutcome::Cleared
    );
    assert_eq!(
        storage
            .clear_initiative(ClearInitiative {
                initiative_id,
                expected_initiative_revision: revision(&storage, initiative_id),
                now: now(),
            })
            .expect("clear the initiative"),
        ClearOutcome::AlreadyClear
    );
}

#[test]
fn a_cleared_initiative_is_no_longer_the_newest_but_is_still_readable() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    storage
        .clear_initiative(ClearInitiative {
            initiative_id,
            expected_initiative_revision: revision(&storage, initiative_id),
            now: now(),
        })
        .expect("clear the initiative");

    let key = project_key();
    let excluding = storage
        .newest_initiative(
            wayfind_core::InitiativeSelector {
                project_key: &key,
                scope: wayfind_core::InitiativeScope::ExcludingClear,
            },
            Consistency::Strong,
        )
        .expect("read the newest initiative");
    let any = storage
        .newest_initiative(
            wayfind_core::InitiativeSelector {
                project_key: &key,
                scope: wayfind_core::InitiativeScope::AnyStatus,
            },
            Consistency::Strong,
        )
        .expect("read the newest initiative");
    assert_eq!(excluding, None);
    assert_eq!(any, Some(initiative_id));
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

const DOCUMENT: &[u8] = b"# Backends\n\nSQLite is one file.\n";

fn attach(storage: &SqliteStorage, ticket_id: TicketId, name: &str) -> wayfind_core::AttachmentId {
    let AllocatedId::Attachment(id) = storage.allocate(IdScope::Attachment).expect("allocate")
    else {
        panic!("expected an attachment id");
    };
    storage
        .store_attachment(
            wayfind_core::StoreAttachment {
                id,
                ticket_id,
                name: AttachmentName::new(name).expect("parse the name"),
                description: "How the stores compare".to_owned(),
                session_id: None,
                now: now(),
            },
            DOCUMENT,
        )
        .expect("store the attachment");
    id
}

#[test]
fn a_document_comes_back_byte_for_byte() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let id = attach(&storage, ticket_id, "backends.md");

    let read = storage
        .read_attachment(id)
        .expect("read the attachment")
        .expect("the attachment is there");
    assert_eq!(read, DOCUMENT);

    let metadata = storage
        .attachment_metadata(id, Consistency::Strong)
        .expect("read the metadata")
        .expect("the attachment is there");
    assert_eq!(metadata.byte_size, DOCUMENT.len() as u64);
    assert_eq!(metadata.name, "backends.md");
}

#[test]
fn bytes_that_are_not_text_survive_the_round_trip() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let AllocatedId::Attachment(id) = storage.allocate(IdScope::Attachment).expect("allocate")
    else {
        panic!("expected an attachment id");
    };
    let raw: &[u8] = &[0xff, 0xfe, 0x00, 0x41];
    storage
        .store_attachment(
            wayfind_core::StoreAttachment {
                id,
                ticket_id,
                name: AttachmentName::new("raw.bin").expect("parse the name"),
                description: "Not text".to_owned(),
                session_id: None,
                now: now(),
            },
            raw,
        )
        .expect("store the attachment");

    assert_eq!(
        storage
            .read_attachment(id)
            .expect("read the attachment")
            .expect("the attachment is there"),
        raw
    );
}

#[test]
fn a_reference_is_added_once_and_dropped_once() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let owner = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let borrower = ticket(&storage, initiative_id, "Ship it", TicketType::Task);
    let id = attach(&storage, owner, "backends.md");

    let add = AddAttachmentReference {
        attachment_id: id,
        ticket_id: borrower,
        now: now(),
    };
    assert_eq!(
        storage
            .add_reference(add.clone())
            .expect("add the reference"),
        ReferenceOutcome::Added
    );
    assert_eq!(
        storage.add_reference(add).expect("add the reference again"),
        ReferenceOutcome::AlreadyPresent
    );

    let remove = RemoveAttachmentReference {
        attachment_id: id,
        ticket_id: borrower,
    };
    assert_eq!(
        storage
            .remove_reference(remove.clone())
            .expect("drop the reference"),
        ReferenceOutcome::Removed
    );
    assert_eq!(
        storage
            .remove_reference(remove)
            .expect("drop the reference again"),
        ReferenceOutcome::NotPresent
    );
}

#[test]
fn a_ticket_cannot_reference_its_own_document() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let ticket_id = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let id = attach(&storage, ticket_id, "backends.md");

    assert_eq!(
        storage
            .add_reference(AddAttachmentReference {
                attachment_id: id,
                ticket_id,
                now: now(),
            })
            .expect("add the reference"),
        ReferenceOutcome::Conflict(ReferenceConflict::TicketOwnsAttachment { ticket_id })
    );
}

#[test]
fn removing_a_document_takes_its_references_with_it() {
    let (_directory, storage) = store();
    let initiative_id = initiative(&storage);
    let owner = ticket(
        &storage,
        initiative_id,
        "Choose a store",
        TicketType::Research,
    );
    let borrower = ticket(&storage, initiative_id, "Ship it", TicketType::Task);
    let id = attach(&storage, owner, "backends.md");
    storage
        .add_reference(AddAttachmentReference {
            attachment_id: id,
            ticket_id: borrower,
            now: now(),
        })
        .expect("add the reference");

    assert_eq!(
        storage
            .remove_attachment(id)
            .expect("remove the attachment"),
        RemoveAttachmentOutcome::Removed {
            references_removed: 1
        }
    );
    assert_eq!(
        storage.remove_attachment(id).expect("remove it again"),
        RemoveAttachmentOutcome::NotFound
    );
    assert!(storage
        .attachment_references(initiative_id, Consistency::Strong)
        .expect("read the references")
        .is_empty());
    assert!(storage
        .attachment_index(initiative_id, Consistency::Strong)
        .expect("read the index")
        .is_empty());
}
