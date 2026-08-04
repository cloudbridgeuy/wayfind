//! The mutable coordination domain, table for table.
//!
//! Nothing here is guarded. These rows are meant to change: a claim is taken and
//! released, a question is resolved and amended, a ticket is updated in place.
//! The discipline that keeps them apart from the graph is that no capability
//! trait reaches across the two, not that the file forbids a write.
//!
//! The run workspace is the v1 machinery keyed by `run_id` rather than by
//! `initiative_id`: same columns, same text fields, same claim and decision
//! meaning, down to the amend column.

/// Every coordination table, in dependency order.
pub const TABLES: [&str; 16] = [
    "sessions",
    "node_claims",
    "runs",
    "questions",
    "question_dependencies",
    "question_claims",
    "decisions",
    "fog_notes",
    "scope_exclusions",
    "run_attachments",
    "attachment_references",
    "tickets",
    "ticket_links",
    "run_revisions",
    "id_sequences",
    "legacy_initiatives",
];

/// The DDL for the coordination domain.
///
/// `ticket_links.node_hash` carries no foreign key on purpose. The link is
/// ticket-side and advisory: a link to a node the graph never held is a
/// data-entry mistake reported when the link is followed, not something the
/// graph is asked to enforce.
pub const DDL: &str = "\
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  initiative_id INTEGER REFERENCES initiatives(id),
  base_ordinal INTEGER,
  status TEXT NOT NULL CHECK(status IN ('active', 'closed')),
  started_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS node_claims (
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  node_hash TEXT NOT NULL,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  base_ordinal INTEGER NOT NULL,
  claimed_at TEXT NOT NULL,
  released_at TEXT,
  PRIMARY KEY (initiative_id, node_hash)
);
CREATE TABLE IF NOT EXISTS runs (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  from_node_hash TEXT NOT NULL,
  destination TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('charting', 'working', 'clear')),
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS questions (
  id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL REFERENCES runs(id),
  title TEXT NOT NULL,
  type TEXT NOT NULL,
  status TEXT NOT NULL,
  question TEXT NOT NULL,
  resolution TEXT,
  created_at TEXT NOT NULL,
  resolved_at TEXT,
  amended_at TEXT,
  UNIQUE(run_id, title)
);
CREATE TABLE IF NOT EXISTS question_dependencies (
  question_id INTEGER NOT NULL REFERENCES questions(id),
  blocker_id INTEGER NOT NULL REFERENCES questions(id),
  PRIMARY KEY (question_id, blocker_id),
  CHECK (question_id <> blocker_id)
);
CREATE TABLE IF NOT EXISTS question_claims (
  question_id INTEGER PRIMARY KEY REFERENCES questions(id),
  session_id TEXT NOT NULL REFERENCES sessions(id),
  claimed_at TEXT NOT NULL,
  released_at TEXT
);
CREATE TABLE IF NOT EXISTS decisions (
  id INTEGER PRIMARY KEY,
  question_id INTEGER NOT NULL UNIQUE REFERENCES questions(id),
  gist TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS fog_notes (
  id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL REFERENCES runs(id),
  note TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS scope_exclusions (
  id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL REFERENCES runs(id),
  note TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS run_attachments (
  id INTEGER PRIMARY KEY,
  question_id INTEGER NOT NULL REFERENCES questions(id),
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  content BLOB NOT NULL,
  byte_size INTEGER NOT NULL,
  session_id TEXT REFERENCES sessions(id),
  created_at TEXT NOT NULL,
  UNIQUE(question_id, name)
);
CREATE TABLE IF NOT EXISTS attachment_references (
  attachment_id INTEGER NOT NULL REFERENCES run_attachments(id),
  question_id INTEGER NOT NULL REFERENCES questions(id),
  created_at TEXT NOT NULL,
  PRIMARY KEY (attachment_id, question_id)
);
CREATE TABLE IF NOT EXISTS tickets (
  id INTEGER PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  priority TEXT NOT NULL CHECK(priority IN ('low', 'normal', 'high', 'urgent')),
  status TEXT NOT NULL CHECK(status IN ('open', 'in-progress', 'done', 'dropped')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS ticket_links (
  ticket_id INTEGER NOT NULL REFERENCES tickets(id),
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  node_hash TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(ticket_id, initiative_id, node_hash)
);
CREATE TABLE IF NOT EXISTS run_revisions (
  run_id INTEGER PRIMARY KEY REFERENCES runs(id),
  revision INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS id_sequences (
  scope TEXT PRIMARY KEY,
  next_id INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS legacy_initiatives (
  initiative_id INTEGER PRIMARY KEY REFERENCES initiatives(id),
  legacy_id INTEGER NOT NULL,
  source_project_key TEXT NOT NULL,
  migrated_at TEXT NOT NULL,
  UNIQUE(source_project_key, legacy_id)
);
";
