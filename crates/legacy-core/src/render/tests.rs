#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::str::FromStr;

use super::{
    render_attachment_header, render_attachment_list, render_csv, render_handoff, render_init,
    render_initiative_cleared, render_map, render_next_unavailable, render_search,
    render_session_list, render_session_resume, render_ticket, state_guidance, AttachmentListView,
    AttachmentRow, AttachmentView, DecisionRow, DumpRow, FrontMatter, FrontierRow, FullDecision,
    HandoffView, InitiativeHeader, MapView, NextView, OwnedAttachmentRow, ReferencedAttachmentRow,
    SearchView, SessionListView, SessionResumeView, SessionRow, TicketView, UnresolvedRow,
};
use crate::id::{AttachmentId, InitiativeId, ProjectKey, SessionId, TicketId};
use crate::model::{
    BlockedReason, FrontierTicket, InitiativeState, NonEmptyVec, PersistedInitiativeStatus,
    TicketStatusLabel, TicketType,
};
use crate::search::SearchHit;
use crate::time::Timestamp;

fn initiative_id() -> InitiativeId {
    InitiativeId::new(3).unwrap()
}

fn ticket_id(value: i64) -> TicketId {
    TicketId::new(value).unwrap()
}

fn attachment_id(value: i64) -> AttachmentId {
    AttachmentId::new(value).unwrap()
}

fn moment() -> Timestamp {
    Timestamp::from_str("2026-08-02 13:45:09").unwrap()
}

fn header() -> InitiativeHeader {
    InitiativeHeader {
        id: initiative_id(),
        name: "Cache the map".to_string(),
        destination: "A map that loads instantly".to_string(),
        notes: String::new(),
        status: PersistedInitiativeStatus::Working,
    }
}

fn ready() -> InitiativeState {
    InitiativeState::Ready {
        frontier: NonEmptyVec::try_from(vec![FrontierTicket {
            id: ticket_id(1),
            title: "Measure the load".to_string(),
            ticket_type: TicketType::Research,
        }])
        .unwrap(),
    }
}

// -- front matter ------------------------------------------------------

#[test]
fn front_matter_keeps_the_order_it_was_built_in() {
    let block = FrontMatter::new("ticket")
        .number("id", 4_i64)
        .text("title", "A \"quoted\" title")
        .ids("blocked_by", vec![ticket_id(2), ticket_id(9)])
        .render();
    assert_eq!(
        block,
        "+++\nkind = \"ticket\"\nid = 4\ntitle = \"A \\\"quoted\\\" title\"\nblocked_by = [2,9]\n+++\n"
    );
}

#[test]
fn an_empty_identifier_list_renders_as_an_empty_list() {
    let block = FrontMatter::new("ticket")
        .ids("blocked_by", Vec::new())
        .render();
    assert!(block.contains("blocked_by = []\n"));
}

#[test]
fn an_absent_optional_key_is_left_out_entirely() {
    let block = FrontMatter::new("ticket")
        .optional_text("amended_at", None::<String>)
        .render();
    assert!(!block.contains("amended_at"));
}

#[test]
fn every_rendered_front_matter_block_parses_as_toml() {
    let awkward = TicketView {
        id: ticket_id(1),
        title: "tab\there and \"quotes\"".to_string(),
        ticket_type: TicketType::Task,
        status: TicketStatusLabel::Open,
        question: "why?".to_string(),
        resolution: None,
        amended_at: None,
        blocked_by: Vec::new(),
        attachments: Vec::new(),
        referenced: Vec::new(),
    };
    let rendered = render_ticket(&awkward);
    let block = rendered
        .split("+++\n")
        .nth(1)
        .expect("a front-matter block");
    let parsed: toml::Table = toml::from_str(block).expect("valid TOML");
    assert_eq!(parsed["title"].as_str(), Some("tab\there and \"quotes\""));
}

// -- guidance ----------------------------------------------------------

#[test]
fn guidance_answers_every_state() {
    assert!(state_guidance(&InitiativeState::Clear, initiative_id()).contains("is clear"));
    assert!(state_guidance(&InitiativeState::Complete, initiative_id())
        .contains("wayfind initiative clear"));
    assert!(state_guidance(&InitiativeState::Charting, initiative_id()).contains("no tickets yet"));
    assert!(state_guidance(&ready(), initiative_id()).contains("wayfind next"));
    assert!(state_guidance(
        &InitiativeState::Blocked(BlockedReason::ClaimsHoldFrontier { claimed: 2 }),
        initiative_id()
    )
    .contains("2 claimed ticket(s)"));
    assert!(state_guidance(
        &InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked),
        initiative_id()
    )
    .contains("wayfind tree"));
}

// -- the map -----------------------------------------------------------

fn map_with(frontier: Vec<FrontierRow>, state: InitiativeState) -> MapView {
    MapView {
        initiative: header(),
        frontier,
        state,
        decisions: Vec::new(),
        fog: Vec::new(),
        exclusions: Vec::new(),
    }
}

#[test]
fn a_map_lists_its_sections_in_order() {
    let mut model = map_with(
        vec![FrontierRow {
            id: ticket_id(1),
            title: "Measure the load".to_string(),
            ticket_type: TicketType::Research,
        }],
        ready(),
    );
    model.decisions.push(DecisionRow {
        ticket_id: ticket_id(2),
        title: "Pick a store".to_string(),
        gist: "SQLite,\nbundled".to_string(),
    });
    model.fog.push("How large does the cache get?".to_string());
    model.exclusions.push("Multi-user access".to_string());
    let rendered = render_map(&model);

    assert_eq!(
        rendered,
        concat!(
            "+++\n",
            "kind = \"map\"\n",
            "initiative_id = 3\n",
            "name = \"Cache the map\"\n",
            "status = \"working\"\n",
            "+++\n",
            "\n",
            "# Cache the map\n\n",
            "## Destination\n\nA map that loads instantly\n\n",
            "## Frontier\n\n",
            "- [1] Measure the load (research)\n",
            "\n## Decisions so far\n\n",
            "Gists are clamped. Run `wayfind ticket ID` for the full decision.\n\n",
            "- [2] Pick a store — SQLite, bundled\n",
            "\n## Not yet specified\n\n",
            "- How large does the cache get?\n",
            "\n## Out of scope\n\n",
            "- Multi-user access\n",
        )
    );
}

#[test]
fn a_map_shows_its_notes_only_when_there_are_notes() {
    let mut model = map_with(Vec::new(), InitiativeState::Charting);
    assert!(!render_map(&model).contains("## Notes"));
    model.initiative.notes = "Read the old design first.".to_string();
    assert!(render_map(&model).contains("## Notes\n\nRead the old design first.\n\n"));
}

#[test]
fn an_empty_frontier_prints_guidance_instead_of_a_list() {
    let model = map_with(Vec::new(), InitiativeState::Charting);
    let rendered = render_map(&model);
    assert!(rendered.contains("## Frontier\n\nInitiative 3 has no tickets yet."));
}

#[test]
fn a_map_keeps_every_row_on_one_line() {
    let mut model = map_with(
        vec![FrontierRow {
            id: ticket_id(1),
            title: "Two\nlines".to_string(),
            ticket_type: TicketType::Task,
        }],
        ready(),
    );
    model.fog.push("Fog\nover\ntwo".to_string());
    let rendered = render_map(&model);
    assert!(rendered.contains("- [1] Two lines (task)\n"));
    assert!(rendered.contains("- Fog over two\n"));
}

#[test]
fn a_long_decision_gist_is_clamped_but_the_handoff_keeps_it_whole() {
    let long = "x".repeat(400);
    let mut model = map_with(Vec::new(), InitiativeState::Complete);
    model.decisions.push(DecisionRow {
        ticket_id: ticket_id(2),
        title: "Long one".to_string(),
        gist: long.clone(),
    });
    assert!(render_map(&model).contains('…'));

    let handoff = HandoffView {
        initiative: header(),
        unresolved: Vec::new(),
        decisions: vec![FullDecision {
            ticket_id: ticket_id(2),
            title: "Long one".to_string(),
            question: "how long?".to_string(),
            resolution: long.clone(),
        }],
        fog: Vec::new(),
        exclusions: Vec::new(),
        attachments: Vec::new(),
    };
    assert!(render_handoff(&handoff).contains(&long));
}

// -- one ticket --------------------------------------------------------

fn ticket_view() -> TicketView {
    TicketView {
        id: ticket_id(4),
        title: "Pick a store".to_string(),
        ticket_type: TicketType::Grilling,
        status: TicketStatusLabel::Open,
        question: "Which store?".to_string(),
        resolution: None,
        amended_at: None,
        blocked_by: Vec::new(),
        attachments: Vec::new(),
        referenced: Vec::new(),
    }
}

#[test]
fn a_plain_ticket_renders_its_question_and_nothing_else() {
    assert_eq!(
        render_ticket(&ticket_view()),
        concat!(
            "+++\n",
            "kind = \"ticket\"\n",
            "id = 4\n",
            "title = \"Pick a store\"\n",
            "type = \"grilling\"\n",
            "status = \"open\"\n",
            "blocked_by = []\n",
            "attachments = 0\n",
            "referenced = 0\n",
            "+++\n",
            "\n",
            "# Pick a store\n\n## Question\n\nWhich store?\n",
        )
    );
}

#[test]
fn a_settled_ticket_adds_its_decision_and_its_repair_time() {
    let mut model = ticket_view();
    model.status = TicketStatusLabel::Resolved;
    model.resolution = Some("SQLite".to_string());
    model.amended_at = Some(moment());
    let rendered = render_ticket(&model);
    assert!(rendered.contains("amended_at = \"2026-08-02 13:45:09\"\n"));
    assert!(rendered.ends_with("\n## Resolution\n\nSQLite\n"));
}

#[test]
fn a_blocked_ticket_names_its_blockers_in_the_front_matter() {
    let mut model = ticket_view();
    model.blocked_by = vec![ticket_id(2), ticket_id(7)];
    assert!(render_ticket(&model).contains("blocked_by = [2,7]\n"));
}

#[test]
fn attachment_sections_appear_only_when_there_are_attachments() {
    let mut model = ticket_view();
    assert!(!render_ticket(&model).contains("## Attachments"));

    model.attachments.push(AttachmentRow {
        id: attachment_id(1),
        name: "notes.md".to_string(),
        bytes: 2048,
        description: "The old design".to_string(),
    });
    model.referenced.push(ReferencedAttachmentRow {
        id: attachment_id(5),
        name: "bench.txt".to_string(),
        bytes: 512,
        owner: ticket_id(9),
        description: "Timings".to_string(),
    });
    let rendered = render_ticket(&model);
    assert!(rendered.contains("attachments = 1\nreferenced = 1\n"));
    assert!(rendered.contains("- [1] notes.md (2.0 KB) — The old design\n"));
    assert!(rendered.contains("- [5] bench.txt (512 B) — from ticket 9 — Timings\n"));
}

// -- the handoff -------------------------------------------------------

#[test]
fn a_handoff_counts_what_it_carries_and_warns_about_open_work() {
    let model = HandoffView {
        initiative: header(),
        unresolved: vec![UnresolvedRow {
            id: ticket_id(1),
            title: "Measure the load".to_string(),
            ticket_type: TicketType::Research,
            status: TicketStatusLabel::Claimed,
        }],
        decisions: vec![FullDecision {
            ticket_id: ticket_id(2),
            title: "Pick a store".to_string(),
            question: "Which store?".to_string(),
            resolution: "SQLite, bundled.".to_string(),
        }],
        fog: vec!["How large?".to_string()],
        exclusions: Vec::new(),
        attachments: vec![OwnedAttachmentRow {
            id: attachment_id(1),
            ticket_id: ticket_id(2),
            name: "bench.txt".to_string(),
            bytes: 1024,
            description: "Timings".to_string(),
        }],
    };
    let rendered = render_handoff(&model);
    assert!(rendered.contains("decisions = 1\nunresolved = 1\nattachments = 1\n"));
    assert!(rendered.contains("# Handoff: Cache the map\n"));
    assert!(rendered.contains("## Unresolved tickets\n"));
    assert!(rendered.contains("- [1] Measure the load (research, claimed)\n"));
    assert!(rendered
        .contains("\n### [2] Pick a store\n\n**Question.** Which store?\n\nSQLite, bundled.\n"));
    assert!(rendered.contains("| 1 | 2 | bench.txt | 1.0 KB | Timings |\n"));
}

#[test]
fn a_clear_handoff_leaves_out_the_unresolved_section() {
    let model = HandoffView {
        initiative: header(),
        unresolved: Vec::new(),
        decisions: Vec::new(),
        fog: Vec::new(),
        exclusions: Vec::new(),
        attachments: Vec::new(),
    };
    let rendered = render_handoff(&model);
    assert!(!rendered.contains("## Unresolved tickets"));
    assert!(!rendered.contains("## Attachments"));
    assert!(rendered.ends_with("## Out of scope\n\n"));
}

// -- tables ------------------------------------------------------------

#[test]
fn a_table_cell_cannot_end_its_row_early() {
    let model = AttachmentListView {
        initiative_id: initiative_id(),
        attachments: vec![OwnedAttachmentRow {
            id: attachment_id(1),
            ticket_id: ticket_id(2),
            name: "a|b.txt".to_string(),
            bytes: 10,
            description: "one | two".to_string(),
        }],
    };
    let rendered = render_attachment_list(&model);
    assert!(rendered.contains("| 1 | 2 | a\\|b.txt | 10 B | one \\| two |\n"));
}

#[test]
fn the_session_table_reports_who_is_holding_what() {
    let model = SessionListView {
        initiative_id: initiative_id(),
        sessions: vec![
            SessionRow {
                id: SessionId::new("session-a").unwrap(),
                holding: Some((ticket_id(4), "Pick a store".to_string())),
                last_seen_at: moment(),
            },
            SessionRow {
                id: SessionId::new("session-b").unwrap(),
                holding: None,
                last_seen_at: moment(),
            },
        ],
    };
    let rendered = render_session_list(&model);
    assert!(rendered.contains("count = 2\n"));
    assert!(rendered.contains("| session-a | working | [4] Pick a store | 2026-08-02 13:45:09 |\n"));
    assert!(rendered.contains("| session-b | ready | — | 2026-08-02 13:45:09 |\n"));
}

#[test]
fn an_empty_session_table_still_prints_its_heading_row() {
    let model = SessionListView {
        initiative_id: initiative_id(),
        sessions: Vec::new(),
    };
    let rendered = render_session_list(&model);
    assert!(rendered.contains("count = 0\n"));
    assert!(rendered.ends_with("| --- | --- | --- | --- |\n"));
}

// -- sessions and next -------------------------------------------------

#[test]
fn a_resumed_session_reports_where_the_initiative_stands() {
    let model = SessionResumeView {
        session_id: SessionId::new("session-a").unwrap(),
        initiative_id: initiative_id(),
        state: InitiativeState::Charting,
    };
    assert_eq!(
        render_session_resume(&model),
        concat!(
            "+++\n",
            "kind = \"session\"\n",
            "id = \"session-a\"\n",
            "initiative_id = 3\n",
            "status = \"charting\"\n",
            "+++\n",
            "\n# Wayfind session\n\n",
            "Initiative 3 has no tickets yet. Chart the map with `wayfind ticket create`.\n",
        )
    );
}

#[test]
fn nothing_available_is_an_answer_rather_than_a_failure() {
    let model = NextView {
        initiative_id: initiative_id(),
        state: InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked),
    };
    let rendered = render_next_unavailable(&model);
    assert!(rendered.contains("kind = \"next\"\ninitiative_id = 3\nstatus = \"blocked\"\n"));
    assert!(rendered.contains("# No available ticket\n\n"));
    assert!(rendered.ends_with("Run `wayfind tree`.\n"));
}

// -- attachments -------------------------------------------------------

#[test]
fn an_attachment_heading_ends_at_the_rule_before_the_document() {
    let model = AttachmentView {
        id: attachment_id(1),
        name: "notes.md".to_string(),
        ticket_id: ticket_id(4),
        bytes: 2048,
        created_at: moment(),
        description: "The old design".to_string(),
    };
    assert_eq!(
        render_attachment_header(&model),
        concat!(
            "+++\n",
            "kind = \"attachment\"\n",
            "id = 1\n",
            "name = \"notes.md\"\n",
            "ticket_id = 4\n",
            "bytes = 2048\n",
            "created_at = \"2026-08-02 13:45:09\"\n",
            "+++\n",
            "\n# notes.md\n\nThe old design\n\n---\n\n",
        )
    );
}

// -- search ------------------------------------------------------------

#[test]
fn search_reports_what_was_asked_and_what_was_found() {
    let model = SearchView {
        query: "cache".to_string(),
        limit: 10,
        offset: 0,
        hits: vec![SearchHit {
            ticket_id: ticket_id(4),
            title: "Pick a store".to_string(),
            status: TicketStatusLabel::Resolved,
            snippet: "a **cache** that\nloads".to_string(),
            metadata: Default::default(),
        }],
    };
    let rendered = render_search(&model);
    assert!(rendered.contains("query = \"cache\"\nlimit = 10\noffset = 0\n"));
    assert!(rendered.contains("- [4] Pick a store (resolved) — a **cache** that loads\n"));
}

// -- records and one-line reports --------------------------------------

#[test]
fn csv_records_carry_a_header_and_survive_awkward_text() {
    let rows = vec![DumpRow {
        id: ticket_id(4),
        title: "Pick a store, \"today\"".to_string(),
        ticket_type: TicketType::Grilling,
        status: TicketStatusLabel::Resolved,
        question: "Which\nstore?".to_string(),
        resolution: String::new(),
    }];
    let rendered = render_csv(&rows).unwrap();

    let mut reader = csv::Reader::from_reader(rendered.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .unwrap()
        .iter()
        .map(str::to_string)
        .collect();
    assert_eq!(
        headers,
        vec!["id", "title", "type", "status", "question", "resolution"]
    );
    let record = reader.records().next().unwrap().unwrap();
    assert_eq!(&record[0], "4");
    assert_eq!(&record[1], "Pick a store, \"today\"");
    assert_eq!(&record[2], "grilling");
    assert_eq!(&record[3], "resolved");
    assert_eq!(&record[4], "Which store?");
    assert_eq!(&record[5], "");
}

#[test]
fn no_records_still_leaves_a_header_to_read() {
    let rendered = render_csv(&[]).unwrap();
    assert_eq!(rendered, "id,title,type,status,question,resolution\n");
}

#[test]
fn the_one_line_reports_say_what_happened() {
    let key = ProjectKey::new("/Users/example/project").unwrap();
    assert_eq!(
        render_init("/Users/example/.config/wayfind/wayfind.sqlite", &key),
        "initialized /Users/example/.config/wayfind/wayfind.sqlite for /Users/example/project\n"
    );
    assert_eq!(
        render_initiative_cleared(initiative_id()),
        "initiative 3 is clear\n"
    );
}
