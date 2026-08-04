//! The immutable graph domain, table for table.
//!
//! Every table here is insert-only. The adapter holds no UPDATE and no DELETE
//! against any of them, and Task 12's guard triggers make that a property of the
//! file rather than a property of this crate's discipline.
//!
//! Two columns the v1 schema had are deliberately absent. `initiatives` has no
//! status, and `snapshots` has no head pointer: lifecycle is derived from the
//! members and the head is the row with the highest ordinal. Neither can drift
//! out of agreement with the records, because neither is written twice.

/// Every immutable table, in dependency order.
///
/// The guard triggers are generated from this list, so a table added here
/// cannot be left unguarded.
pub const TABLES: [&str; 13] = [
    "projects",
    "initiatives",
    "result_nodes",
    "transitions",
    "transition_inputs",
    "transition_outputs",
    "connections",
    "artifacts",
    "artifact_references",
    "import_provenance",
    "import_members",
    "snapshots",
    "root_members",
];

/// The DDL for the immutable domain.
///
/// `from_hash`, `to_hash`, and `owner_hash` carry no foreign key: each can name
/// a record of more than one kind, and SQLite has no way to say that. Acceptance
/// validation proves the referent exists inside the same transaction.
pub const DDL: &str = "\
CREATE TABLE IF NOT EXISTS projects (
  key TEXT PRIMARY KEY,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS initiatives (
  id INTEGER PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  name TEXT NOT NULL,
  destination TEXT NOT NULL,
  notes TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(project_key, name)
);
CREATE TABLE IF NOT EXISTS result_nodes (
  hash TEXT PRIMARY KEY,
  node_kind TEXT NOT NULL,
  title TEXT NOT NULL,
  summary TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  created_by TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS transitions (
  hash TEXT PRIMARY KEY,
  transition_kind TEXT NOT NULL,
  summary TEXT NOT NULL,
  rationale TEXT,
  created_at TEXT NOT NULL,
  created_by TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS transition_inputs (
  transition_hash TEXT NOT NULL REFERENCES transitions(hash),
  node_hash TEXT NOT NULL REFERENCES result_nodes(hash),
  position INTEGER NOT NULL,
  PRIMARY KEY (transition_hash, node_hash)
);
CREATE TABLE IF NOT EXISTS transition_outputs (
  transition_hash TEXT NOT NULL REFERENCES transitions(hash),
  node_hash TEXT NOT NULL REFERENCES result_nodes(hash),
  position INTEGER NOT NULL,
  PRIMARY KEY (transition_hash, node_hash)
);
CREATE TABLE IF NOT EXISTS connections (
  hash TEXT PRIMARY KEY,
  transition_hash TEXT NOT NULL REFERENCES transitions(hash),
  from_hash TEXT NOT NULL,
  to_hash TEXT NOT NULL,
  relation TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS artifacts (
  hash TEXT PRIMARY KEY,
  content BLOB NOT NULL,
  content_hash TEXT NOT NULL,
  byte_size INTEGER NOT NULL,
  description TEXT NOT NULL,
  created_at TEXT NOT NULL,
  created_by TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS artifact_references (
  owner_hash TEXT NOT NULL,
  artifact_hash TEXT NOT NULL REFERENCES artifacts(hash),
  PRIMARY KEY (owner_hash, artifact_hash)
);
CREATE TABLE IF NOT EXISTS import_provenance (
  transition_hash TEXT PRIMARY KEY REFERENCES transitions(hash),
  source_initiative_id INTEGER NOT NULL,
  source_snapshot_ordinal INTEGER NOT NULL,
  rationale TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS import_members (
  transition_hash TEXT NOT NULL REFERENCES transitions(hash),
  record_kind TEXT NOT NULL,
  record_hash TEXT NOT NULL,
  PRIMARY KEY (transition_hash, record_kind, record_hash)
);
CREATE TABLE IF NOT EXISTS snapshots (
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  ordinal INTEGER NOT NULL,
  transition_hash TEXT REFERENCES transitions(hash),
  declared_base_ordinal INTEGER,
  chain_hash TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (initiative_id, ordinal)
);
CREATE TABLE IF NOT EXISTS root_members (
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  record_kind TEXT NOT NULL,
  record_hash TEXT NOT NULL,
  PRIMARY KEY (initiative_id, record_kind, record_hash)
);
";
