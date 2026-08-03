//! Semantic parity checks for every kind of document Wayfind prints.
//!
//! These tests exist because the Rust port has to keep agreeing with the Bash
//! script it replaces, and because a snapshot of prose is the wrong instrument
//! for that. A snapshot fails when a sentence is reworded, which is allowed, and
//! passes when a key silently changes meaning, which is not.
//!
//! So each document is parsed back into the thing a reader actually depends on
//! and compared as data:
//!
//! - front matter as key/value pairs, with the keys and their values checked;
//! - Markdown by heading order, so a reader that skips to `## Resolution` keeps
//!   finding it in the same place;
//! - tables and lists as rows and items;
//! - `dump --csv` as records, through a real CSV reader;
//! - attachment content as exact bytes, which is the one place where nothing
//!   may be reinterpreted.
//!
//! What is deliberately *not* asserted is wording. Guidance sentences, table
//! captions, and the note above the decisions may all be rewritten without
//! touching these tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use crate::format::strip_one_trailing_newline;
use crate::id::{AttachmentId, InitiativeId, ProjectKey, SessionId, TicketId};
use crate::model::{
    BlockedReason, Dependency, InitiativeState, NonEmptyVec, PersistedClaim,
    PersistedInitiativeStatus, PersistedTicketState, Ticket, TicketState, TicketStatusLabel,
    TicketType,
};
use crate::render::{
    render_attachment_header, render_attachment_list, render_csv, render_handoff, render_init,
    render_initiative_cleared, render_map, render_next_unavailable, render_search,
    render_session_list, render_session_resume, render_ticket, AttachmentListView, AttachmentRow,
    AttachmentView, DecisionRow, DumpRow, FrontierRow, FullDecision, HandoffView, InitiativeHeader,
    MapView, NextView, OwnedAttachmentRow, ReferencedAttachmentRow, SearchView, SessionListView,
    SessionResumeView, SessionRow, TicketView, UnresolvedRow, DUMP_HEADER,
};
use crate::search::SearchHit;
use crate::time::Timestamp;
use crate::tree::{render_tree, TreeView};

// ---------------------------------------------------------------------------
// Readers
//
// One reader per shape a consumer can depend on. They are deliberately strict:
// a document that does not have the shape panics here rather than quietly
// comparing equal to nothing.
// ---------------------------------------------------------------------------

/// The `+++` block, as key/value pairs in the order they were written.
///
/// Values keep their TOML spelling — `1`, `"text"`, `[1,2]` — because the point
/// of the check is that the spelling is stable, not only the meaning.
fn front_matter(document: &str) -> Vec<(String, String)> {
    let mut lines = document.lines();
    assert_eq!(lines.next(), Some("+++"), "document has no front matter");

    let mut pairs = Vec::new();
    for line in lines {
        if line == "+++" {
            return pairs;
        }
        let (key, value) = line
            .split_once(" = ")
            .unwrap_or_else(|| panic!("front-matter line is not `key = value`: {line}"));
        pairs.push((key.to_string(), value.to_string()));
    }
    panic!("front matter is not closed");
}

/// The front matter as a map, for looking one key up.
fn keyed(document: &str) -> BTreeMap<String, String> {
    front_matter(document).into_iter().collect()
}

/// Just the keys, in order.
fn keys(document: &str) -> Vec<String> {
    front_matter(document)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

/// The text of a TOML basic string, with its escapes undone.
fn unquoted(value: &str) -> String {
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| panic!("value is not a quoted string: {value}"));

    let mut out = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next().expect("escape at end of string") {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'u' => {
                let digits: String = characters.by_ref().take(4).collect();
                let point = u32::from_str_radix(&digits, 16).expect("escape is not hexadecimal");
                out.push(char::from_u32(point).expect("escape is not a character"));
            }
            other => panic!("unknown escape: \\{other}"),
        }
    }
    out
}

/// Every ATX heading, in the order it appears, level marks included.
///
/// This is the structure a reader navigates by, so its order is part of the
/// contract even though the prose underneath it is not.
fn headings(document: &str) -> Vec<String> {
    document
        .lines()
        .filter(|line| line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The lines between one heading and the next, with blank lines dropped.
fn section(document: &str, heading: &str) -> Vec<String> {
    let mut lines = document.lines().skip_while(|line| *line != heading);
    assert!(lines.next().is_some(), "no heading {heading}");
    lines
        .take_while(|line| !line.starts_with('#'))
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// The list items of a section, without their `- ` marks.
fn items(document: &str, heading: &str) -> Vec<String> {
    section(document, heading)
        .into_iter()
        .filter_map(|line| line.strip_prefix("- ").map(str::to_string))
        .collect()
}

/// A Markdown table's body, as cells. The header and rule rows are dropped.
///
/// Splitting honours the `\|` escape, so a cell holding a pipe stays one cell —
/// which is exactly the fault this escape was added to prevent.
fn table(document: &str) -> Vec<Vec<String>> {
    document
        .lines()
        .filter(|line| line.starts_with("| "))
        .skip(2)
        .map(|line| {
            let trimmed = line.trim_matches('|');
            let mut cells = Vec::new();
            let mut cell = String::new();
            let mut characters = trimmed.chars();
            while let Some(character) = characters.next() {
                match character {
                    '\\' => cell.push(characters.next().unwrap_or('\\')),
                    '|' => cells.push(std::mem::take(&mut cell).trim().to_string()),
                    plain => cell.push(plain),
                }
            }
            cells.push(cell.trim().to_string());
            cells
        })
        .collect()
}

/// The records of a CSV document, header included, read by a real CSV reader.
fn records(text: &str) -> Vec<Vec<String>> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(text.as_bytes())
        .records()
        .map(|record| {
            record
                .expect("record is not readable")
                .iter()
                .map(str::to_string)
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn moment() -> Timestamp {
    "2026-04-01 09:30:00".parse().unwrap()
}

fn initiative_id() -> InitiativeId {
    InitiativeId::new(7).unwrap()
}

fn ticket_id(id: i64) -> TicketId {
    TicketId::new(id).unwrap()
}

fn attachment_id(id: i64) -> AttachmentId {
    AttachmentId::new(id).unwrap()
}

/// A header carrying a quotation mark, so front-matter escaping is exercised
/// by every document that repeats the initiative's name.
fn header() -> InitiativeHeader {
    InitiativeHeader {
        id: initiative_id(),
        name: "Port the \"wayfind\" script".to_string(),
        destination: "One binary, same database".to_string(),
        notes: "Runs against the live schema".to_string(),
        status: PersistedInitiativeStatus::Working,
    }
}

/// A ready state with two tickets on the frontier.
fn ready() -> InitiativeState {
    InitiativeState::Ready {
        frontier: NonEmptyVec::try_from(vec![
            crate::model::FrontierTicket {
                id: ticket_id(1),
                title: "Chart the map".to_string(),
                ticket_type: TicketType::Research,
            },
            crate::model::FrontierTicket {
                id: ticket_id(2),
                title: "Port the shell".to_string(),
                ticket_type: TicketType::Task,
            },
        ])
        .unwrap(),
    }
}

fn map_view() -> MapView {
    MapView {
        initiative: header(),
        frontier: vec![
            FrontierRow {
                id: ticket_id(1),
                title: "Chart the map".to_string(),
                ticket_type: TicketType::Research,
            },
            FrontierRow {
                id: ticket_id(2),
                title: "Port the\nshell".to_string(),
                ticket_type: TicketType::Task,
            },
        ],
        state: ready(),
        decisions: vec![DecisionRow {
            ticket_id: ticket_id(3),
            title: "Pick a storage boundary".to_string(),
            gist: "Capability traits, not SQL".to_string(),
        }],
        fog: vec!["Windows paths are untested".to_string()],
        exclusions: vec!["A web interface".to_string()],
    }
}

fn ticket_view() -> TicketView {
    TicketView {
        id: ticket_id(3),
        title: "Pick a storage boundary".to_string(),
        ticket_type: TicketType::Grilling,
        status: TicketStatusLabel::Resolved,
        question: "How does the core reach the store?".to_string(),
        resolution: "Capability traits, not SQL.".to_string().into(),
        amended_at: Some(moment()),
        blocked_by: vec![ticket_id(1), ticket_id(2)],
        attachments: vec![AttachmentRow {
            id: attachment_id(11),
            name: "bench.txt".to_string(),
            bytes: 2048,
            description: "Timings".to_string(),
        }],
        referenced: vec![ReferencedAttachmentRow {
            id: attachment_id(12),
            name: "schema.sql".to_string(),
            bytes: 640,
            owner: ticket_id(1),
            description: "The shared schema".to_string(),
        }],
    }
}

fn handoff_view() -> HandoffView {
    HandoffView {
        initiative: header(),
        unresolved: vec![UnresolvedRow {
            id: ticket_id(2),
            title: "Port the shell".to_string(),
            ticket_type: TicketType::Task,
            status: TicketStatusLabel::Claimed,
        }],
        decisions: vec![FullDecision {
            ticket_id: ticket_id(3),
            title: "Pick a storage boundary".to_string(),
            question: "How does the core reach the store?".to_string(),
            resolution: "Capability traits, not SQL.\n\nA second paragraph survives.".to_string(),
        }],
        fog: vec!["Windows paths are untested".to_string()],
        exclusions: vec!["A web interface".to_string()],
        attachments: vec![OwnedAttachmentRow {
            id: attachment_id(11),
            ticket_id: ticket_id(3),
            name: "bench.txt".to_string(),
            bytes: 2048,
            description: "Timings".to_string(),
        }],
    }
}

/// One ticket in the given lifecycle position, built the way the store builds
/// it: from the columns, through the same parser.
fn ticket(id: i64, status: &str) -> Ticket {
    let persisted = match status {
        "claimed" => PersistedTicketState {
            status,
            live_claim: Some(PersistedClaim {
                session_id: "agent-42",
                claimed_at: "2026-04-01 09:30:00",
            }),
            ..PersistedTicketState::default()
        },
        "resolved" => PersistedTicketState {
            status,
            resolution: Some("Settled."),
            resolved_at: Some("2026-04-01 09:30:00"),
            ..PersistedTicketState::default()
        },
        _ => PersistedTicketState {
            status,
            ..PersistedTicketState::default()
        },
    };

    Ticket {
        id: ticket_id(id),
        initiative_id: initiative_id(),
        title: format!("Ticket {id}"),
        ticket_type: TicketType::Task,
        question: String::new(),
        state: TicketState::from_persisted(persisted).unwrap(),
        created_at: moment(),
    }
}

// ---------------------------------------------------------------------------
// The map
// ---------------------------------------------------------------------------

#[test]
fn the_map_names_its_kind_initiative_and_status() {
    let document = render_map(&map_view());
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "map");
    assert_eq!(fields["initiative_id"], "7");
    assert_eq!(unquoted(&fields["name"]), "Port the \"wayfind\" script");
    assert_eq!(unquoted(&fields["status"]), "working");
    assert_eq!(keys(&document), ["kind", "initiative_id", "name", "status"]);
}

#[test]
fn the_map_keeps_its_section_order() {
    let document = render_map(&map_view());
    assert_eq!(
        headings(&document),
        [
            "# Port the \"wayfind\" script",
            "## Destination",
            "## Notes",
            "## Frontier",
            "## Decisions so far",
            "## Not yet specified",
            "## Out of scope",
        ]
    );
}

#[test]
fn a_map_without_notes_drops_that_section_and_keeps_the_rest() {
    let mut view = map_view();
    view.initiative.notes = String::new();
    let document = render_map(&view);

    assert!(!headings(&document).contains(&"## Notes".to_string()));
    assert_eq!(
        headings(&document),
        [
            "# Port the \"wayfind\" script",
            "## Destination",
            "## Frontier",
            "## Decisions so far",
            "## Not yet specified",
            "## Out of scope",
        ]
    );
}

#[test]
fn every_frontier_ticket_is_one_item_however_the_title_was_written() {
    let document = render_map(&map_view());
    assert_eq!(
        items(&document, "## Frontier"),
        ["[1] Chart the map (research)", "[2] Port the shell (task)"]
    );
}

#[test]
fn an_empty_frontier_says_what_to_do_instead_of_listing_nothing() {
    let mut view = map_view();
    view.frontier.clear();
    view.state = InitiativeState::Complete;
    let document = render_map(&view);

    let frontier = section(&document, "## Frontier");
    assert_eq!(frontier.len(), 1, "guidance should be one line");
    assert!(
        !frontier[0].starts_with("- "),
        "guidance is not a list item"
    );
    assert!(frontier[0].contains("Initiative 7"));
}

#[test]
fn notes_and_exclusions_are_separate_lists() {
    let document = render_map(&map_view());
    assert_eq!(
        items(&document, "## Not yet specified"),
        ["Windows paths are untested"]
    );
    assert_eq!(items(&document, "## Out of scope"), ["A web interface"]);
}

// ---------------------------------------------------------------------------
// One ticket
// ---------------------------------------------------------------------------

#[test]
fn a_ticket_reports_its_counts_and_blockers_in_front_matter() {
    let document = render_ticket(&ticket_view());
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "ticket");
    assert_eq!(fields["id"], "3");
    assert_eq!(unquoted(&fields["type"]), "grilling");
    assert_eq!(unquoted(&fields["status"]), "resolved");
    assert_eq!(fields["blocked_by"], "[1,2]");
    assert_eq!(fields["attachments"], "1");
    assert_eq!(fields["referenced"], "1");
    assert_eq!(unquoted(&fields["amended_at"]), "2026-04-01 09:30:00");
}

#[test]
fn an_unamended_ticket_omits_the_amended_key_rather_than_emptying_it() {
    let mut view = ticket_view();
    view.amended_at = None;
    let document = render_ticket(&view);

    assert!(!keys(&document).contains(&"amended_at".to_string()));
}

#[test]
fn a_ticket_without_blockers_still_carries_the_key_as_an_empty_list() {
    let mut view = ticket_view();
    view.blocked_by.clear();
    let document = render_ticket(&view);

    assert_eq!(keyed(&document)["blocked_by"], "[]");
}

#[test]
fn a_resolved_ticket_puts_its_decision_after_its_question() {
    let document = render_ticket(&ticket_view());
    assert_eq!(
        headings(&document),
        [
            "# Pick a storage boundary",
            "## Question",
            "## Resolution",
            "## Attachments",
            "## Referenced attachments",
        ]
    );
    assert_eq!(
        section(&document, "## Resolution"),
        ["Capability traits, not SQL."]
    );
}

#[test]
fn an_open_ticket_has_no_resolution_section() {
    let mut view = ticket_view();
    view.resolution = None;
    view.status = TicketStatusLabel::Open;
    view.attachments.clear();
    view.referenced.clear();
    let document = render_ticket(&view);

    assert_eq!(
        headings(&document),
        ["# Pick a storage boundary", "## Question"]
    );
}

#[test]
fn owned_and_referenced_attachments_are_told_apart() {
    let document = render_ticket(&ticket_view());

    assert_eq!(
        items(&document, "## Attachments"),
        ["[11] bench.txt (2.0 KB) — Timings"]
    );
    assert_eq!(
        items(&document, "## Referenced attachments"),
        ["[12] schema.sql (640 B) — from ticket 1 — The shared schema"]
    );
}

// ---------------------------------------------------------------------------
// Next and session
// ---------------------------------------------------------------------------

#[test]
fn an_unavailable_next_is_a_document_and_reports_the_state() {
    let document = render_next_unavailable(&NextView {
        initiative_id: initiative_id(),
        state: InitiativeState::Blocked(BlockedReason::EveryOpenTicketIsBlocked),
    });
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "next");
    assert_eq!(fields["initiative_id"], "7");
    assert_eq!(unquoted(&fields["status"]), "blocked");
    assert_eq!(headings(&document), ["# No available ticket"]);
}

#[test]
fn every_state_gives_next_a_status_word_and_some_guidance() {
    let states = [
        (InitiativeState::Charting, "charting"),
        (ready(), "ready"),
        (
            InitiativeState::Blocked(BlockedReason::ClaimsHoldFrontier { claimed: 2 }),
            "blocked",
        ),
        (InitiativeState::Complete, "complete"),
        (InitiativeState::Clear, "clear"),
    ];

    for (state, word) in states {
        let document = render_next_unavailable(&NextView {
            initiative_id: initiative_id(),
            state,
        });
        assert_eq!(unquoted(&keyed(&document)["status"]), word);
        assert!(
            !section(&document, "# No available ticket").is_empty(),
            "state {word} says nothing"
        );
    }
}

#[test]
fn a_resumed_session_reports_who_it_is_and_where_it_is() {
    let document = render_session_resume(&SessionResumeView {
        session_id: SessionId::new("agent-42").unwrap(),
        initiative_id: initiative_id(),
        state: InitiativeState::Charting,
    });
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "session");
    assert_eq!(unquoted(&fields["id"]), "agent-42");
    assert_eq!(fields["initiative_id"], "7");
    assert_eq!(unquoted(&fields["status"]), "charting");
    assert_eq!(headings(&document), ["# Wayfind session"]);
}

#[test]
fn the_session_table_tells_a_working_session_from_a_ready_one() {
    let document = render_session_list(&SessionListView {
        initiative_id: initiative_id(),
        sessions: vec![
            SessionRow {
                id: SessionId::new("agent-42").unwrap(),
                holding: Some((ticket_id(2), "Port the shell".to_string())),
                last_seen_at: moment(),
            },
            SessionRow {
                id: SessionId::new("agent-7").unwrap(),
                holding: None,
                last_seen_at: moment(),
            },
        ],
    });

    assert_eq!(keyed(&document)["count"], "2");
    assert_eq!(
        table(&document),
        [
            [
                "agent-42",
                "working",
                "[2] Port the shell",
                "2026-04-01 09:30:00"
            ],
            ["agent-7", "ready", "—", "2026-04-01 09:30:00"],
        ]
    );
}

#[test]
fn a_pipe_in_a_title_stays_inside_its_own_cell() {
    let document = render_session_list(&SessionListView {
        initiative_id: initiative_id(),
        sessions: vec![SessionRow {
            id: SessionId::new("agent-42").unwrap(),
            holding: Some((ticket_id(2), "Parse a | b".to_string())),
            last_seen_at: moment(),
        }],
    });

    let rows = table(&document);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 4, "the pipe split the row");
    assert_eq!(rows[0][2], "[2] Parse a | b");
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

#[test]
fn the_attachment_table_is_one_row_per_document() {
    let document = render_attachment_list(&AttachmentListView {
        initiative_id: initiative_id(),
        attachments: vec![
            OwnedAttachmentRow {
                id: attachment_id(11),
                ticket_id: ticket_id(3),
                name: "bench.txt".to_string(),
                bytes: 2048,
                description: "Timings".to_string(),
            },
            OwnedAttachmentRow {
                id: attachment_id(12),
                ticket_id: ticket_id(1),
                name: "schema.sql".to_string(),
                bytes: 640,
                description: "The shared schema".to_string(),
            },
        ],
    });
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "attachments");
    assert_eq!(fields["count"], "2");
    assert_eq!(
        table(&document),
        [
            ["11", "3", "bench.txt", "2.0 KB", "Timings"],
            ["12", "1", "schema.sql", "640 B", "The shared schema"],
        ]
    );
}

#[test]
fn an_empty_attachment_table_is_a_header_and_no_rows() {
    let document = render_attachment_list(&AttachmentListView {
        initiative_id: initiative_id(),
        attachments: Vec::new(),
    });

    assert_eq!(keyed(&document)["count"], "0");
    assert!(table(&document).is_empty());
}

#[test]
fn the_attachment_header_ends_at_a_rule_so_the_content_follows_untouched() {
    let document = render_attachment_header(&AttachmentView {
        id: attachment_id(11),
        name: "bench.txt".to_string(),
        ticket_id: ticket_id(3),
        bytes: 27,
        created_at: moment(),
        description: "Timings".to_string(),
    });
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "attachment");
    assert_eq!(fields["bytes"], "27");
    assert_eq!(unquoted(&fields["created_at"]), "2026-04-01 09:30:00");
    assert_eq!(headings(&document), ["# bench.txt"]);
    assert!(
        document.ends_with("---\n\n"),
        "the header must end at the rule"
    );
}

/// The one exact comparison in this file.
///
/// Everything else is compared as meaning. A stored document is compared as
/// bytes, because `attach show --raw > file` has to give back the file that was
/// filed — including invalid UTF-8, and including a bare carriage return.
///
/// Storing drops one trailing line feed and printing puts one back, so a file
/// that ends in a line feed — which is every ordinary text file — survives the
/// round trip unchanged. Nothing else in the document is touched at all.
#[test]
fn a_newline_terminated_document_comes_back_byte_for_byte() {
    let sources: [&[u8]; 5] = [
        b"benchmark rows\nsecond line\n",
        b"trailing blank line\n\n",
        b"carriage\r\nreturns\r\n",
        b"\n",
        // Not valid UTF-8. The store and the printer both carry bytes, so this
        // is a document like any other.
        &[0xff, 0xfe, b'\n'],
    ];

    for source in sources {
        // What the shell stores, and what it prints back afterwards.
        let mut printed = strip_one_trailing_newline(source).to_vec();
        printed.push(b'\n');

        assert_eq!(printed, source.to_vec(), "round trip changed {source:?}");
    }
}

/// The one document the round trip does change, and by exactly how much.
///
/// A file with no final line feed gains one. This is the script's behaviour and
/// the reason the strip is one byte and not a trim.
#[test]
fn a_document_without_a_final_newline_gains_exactly_one() {
    let source: &[u8] = b"no trailing newline";
    let mut printed = strip_one_trailing_newline(source).to_vec();
    printed.push(b'\n');

    assert_eq!(printed, b"no trailing newline\n".to_vec());
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn search_repeats_the_question_it_was_asked() {
    let document = render_search(&SearchView {
        query: "storage boundary".to_string(),
        limit: 10,
        offset: 20,
        hits: vec![SearchHit {
            ticket_id: ticket_id(3),
            title: "Pick a storage boundary".to_string(),
            status: TicketStatusLabel::Resolved,
            snippet: "Capability **traits**, not SQL".to_string(),
            metadata: BTreeMap::new(),
        }],
    });
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "search");
    assert_eq!(unquoted(&fields["query"]), "storage boundary");
    assert_eq!(fields["limit"], "10");
    assert_eq!(fields["offset"], "20");
    assert_eq!(
        items(&document, "# Search results"),
        ["[3] Pick a storage boundary (resolved) — Capability **traits**, not SQL"]
    );
}

#[test]
fn a_search_with_no_hits_is_still_a_document() {
    let document = render_search(&SearchView {
        query: "nothing here".to_string(),
        limit: 10,
        offset: 0,
        hits: Vec::new(),
    });

    assert_eq!(headings(&document), ["# Search results"]);
    assert!(items(&document, "# Search results").is_empty());
}

// ---------------------------------------------------------------------------
// The handoff
// ---------------------------------------------------------------------------

#[test]
fn the_handoff_counts_what_it_carries() {
    let document = render_handoff(&handoff_view());
    let fields = keyed(&document);

    assert_eq!(unquoted(&fields["kind"]), "handoff");
    assert_eq!(fields["decisions"], "1");
    assert_eq!(fields["unresolved"], "1");
    assert_eq!(fields["attachments"], "1");
}

#[test]
fn the_handoff_keeps_its_section_order_and_one_heading_per_decision() {
    let document = render_handoff(&handoff_view());
    assert_eq!(
        headings(&document),
        [
            "# Handoff: Port the \"wayfind\" script",
            "## Destination",
            "## Notes",
            "## Unresolved tickets",
            "## Decisions",
            "### [3] Pick a storage boundary",
            "## Not yet specified",
            "## Out of scope",
            "## Attachments",
        ]
    );
}

#[test]
fn a_clear_handoff_drops_the_unresolved_section() {
    let mut view = handoff_view();
    view.unresolved.clear();
    let document = render_handoff(&view);

    assert!(!headings(&document).contains(&"## Unresolved tickets".to_string()));
}

#[test]
fn the_handoff_prints_a_decision_in_full_rather_than_as_a_gist() {
    let document = render_handoff(&handoff_view());
    let decision = section(&document, "### [3] Pick a storage boundary");

    assert_eq!(
        decision,
        [
            "**Question.** How does the core reach the store?",
            "Capability traits, not SQL.",
            "A second paragraph survives.",
        ]
    );
}

// ---------------------------------------------------------------------------
// Records and one-line reports
// ---------------------------------------------------------------------------

#[test]
fn the_dump_is_readable_as_records_whatever_the_text_holds() {
    let text = render_csv(&[
        DumpRow {
            id: ticket_id(1),
            title: "Titles, with commas".to_string(),
            ticket_type: TicketType::Research,
            status: TicketStatusLabel::Resolved,
            question: "Does a \"quotation mark\" survive?".to_string(),
            resolution: "It does.\nEven over two lines.".to_string(),
        },
        DumpRow {
            id: ticket_id(2),
            title: "Open one".to_string(),
            ticket_type: TicketType::Task,
            status: TicketStatusLabel::Open,
            question: "Pending".to_string(),
            resolution: String::new(),
        },
    ])
    .unwrap();

    let rows = records(&text);
    assert_eq!(rows[0], DUMP_HEADER);
    assert_eq!(rows.len(), 3, "one header and two records");
    assert_eq!(
        rows[1],
        [
            "1",
            "Titles, with commas",
            "research",
            "resolved",
            "Does a \"quotation mark\" survive?",
            "It does. Even over two lines.",
        ]
    );
    assert_eq!(rows[2][5], "", "an open ticket has no decision");
}

#[test]
fn an_empty_dump_is_a_header_row_on_its_own() {
    let text = render_csv(&[]).unwrap();
    assert_eq!(records(&text), [DUMP_HEADER]);
}

#[test]
fn the_one_line_reports_name_what_they_did() {
    let key = ProjectKey::new("/work/wayfind").unwrap();
    assert_eq!(
        render_init("/tmp/wayfind.sqlite", &key),
        "initialized /tmp/wayfind.sqlite for /work/wayfind\n"
    );
    assert_eq!(
        render_initiative_cleared(initiative_id()),
        "initiative 7 is clear\n"
    );
}

// ---------------------------------------------------------------------------
// The graph snapshot
// ---------------------------------------------------------------------------

#[test]
fn the_tree_draws_one_line_per_ticket_with_its_mark_and_blockers() {
    let tickets = vec![
        ticket(1, "resolved"),
        ticket(2, "claimed"),
        ticket(3, "open"),
    ];
    let dependencies = vec![
        Dependency::new(ticket_id(2), ticket_id(1)).unwrap(),
        Dependency::new(ticket_id(3), ticket_id(2)).unwrap(),
    ];
    let document = render_tree(&TreeView::new("Port the script", &tickets, &dependencies));

    assert_eq!(headings(&document), ["# Port the script"]);

    let drawn: Vec<&str> = document
        .lines()
        .filter(|line| line.contains(" · "))
        .collect();
    assert_eq!(drawn.len(), 3, "one line per ticket");

    // Ticket 3 waits on 2, which waits on 1. The diagram draws what waits
    // first and what it waits on underneath, so the deepest blocker is last.
    for (line, id) in drawn.iter().zip(["[3]", "[2]", "[1]"]) {
        assert!(line.contains(id), "{line} does not name {id}");
    }
    assert!(drawn[0].contains('*'), "an open ticket is starred");
    assert!(drawn[1].contains('▶'), "a claimed ticket is pointed at");
    assert!(drawn[2].contains('✓'), "a resolved ticket is ticked");
    // Each blocked ticket names what it waits on, on its own line under it.
    let forks: Vec<&str> = document
        .lines()
        .filter(|line| line.contains("-> ["))
        .collect();
    assert_eq!(forks.len(), 2, "one fork per blocked ticket");
    assert!(forks[0].ends_with("-> [2]"), "{}", forks[0]);
    assert!(forks[1].ends_with("-> [1]"), "{}", forks[1]);
}

#[test]
fn a_tree_drops_an_edge_that_names_a_ticket_it_cannot_draw() {
    let tickets = vec![ticket(1, "open")];
    let dependencies = vec![Dependency::new(ticket_id(1), ticket_id(99)).unwrap()];
    let document = render_tree(&TreeView::new("Port the script", &tickets, &dependencies));

    assert!(!document.contains("99"), "a dangling edge was drawn");
    assert!(document.contains("[1]"));
}

#[test]
fn an_empty_tree_is_a_heading_and_nothing_else() {
    let document = render_tree(&TreeView::new("Port the script", &[], &[]));
    assert_eq!(headings(&document), ["# Port the script"]);
    assert!(!document.contains(" · "));
}
