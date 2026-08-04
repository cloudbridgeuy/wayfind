-- A database exactly as the Bash script at commit 0120d61 would leave it.
--
-- The schema below is copied from that script's `ensure_database`, character
-- for character, including the `amended_at` column its `migrate_database` adds
-- afterwards. Nothing here is the Rust port's idea of the schema: if the port
-- and this file ever disagree, this file is right and the port is wrong.
--
-- The rows are chosen to be awkward on purpose. Text spans several lines and
-- carries quotes, tabs, and non-Latin characters. There is a dependency chain,
-- a live claim, a released claim, a recorded decision, an amended decision, an
-- excluded ticket, an attachment, and a cross-ticket reference. Anything that
-- reads this database has to cope with all of it.

-- The script also sets `PRAGMA journal_mode = WAL`. That statement answers with
-- a row, which a batch of statements cannot carry, so the test that builds this
-- fixture applies it separately before running the rest of this file.
PRAGMA foreign_keys = ON;

-- --------------------------------------------------------------------------
-- Schema
-- --------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS projects (
  key TEXT PRIMARY KEY,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS initiatives (
  id INTEGER PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  name TEXT NOT NULL,
  destination TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'charting' CHECK(status IN ('charting', 'working', 'clear')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(project_key, name)
);
CREATE TABLE IF NOT EXISTS tickets (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id),
  title TEXT NOT NULL,
  type TEXT NOT NULL CHECK(type IN ('grilling', 'research', 'prototype', 'task')),
  status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open', 'claimed', 'resolved', 'excluded')),
  question TEXT NOT NULL,
  resolution TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  resolved_at TEXT,
  UNIQUE(initiative_id, title)
);
CREATE TABLE IF NOT EXISTS ticket_dependencies (
  ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  blocker_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  PRIMARY KEY(ticket_id, blocker_id),
  CHECK(ticket_id != blocker_id)
);
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  project_key TEXT NOT NULL REFERENCES projects(key),
  initiative_id INTEGER REFERENCES initiatives(id),
  current_ticket_id INTEGER REFERENCES tickets(id),
  resolved_non_research_count INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'closed')),
  started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS ticket_claims (
  ticket_id INTEGER PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  claimed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  released_at TEXT
);
CREATE TABLE IF NOT EXISTS decisions (
  id INTEGER PRIMARY KEY,
  ticket_id INTEGER NOT NULL UNIQUE REFERENCES tickets(id),
  gist TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS fog_notes (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id) ON DELETE CASCADE,
  note TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS scope_exclusions (
  id INTEGER PRIMARY KEY,
  initiative_id INTEGER NOT NULL REFERENCES initiatives(id) ON DELETE CASCADE,
  note TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS attachments (
  id INTEGER PRIMARY KEY,
  ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  content TEXT NOT NULL,
  byte_size INTEGER NOT NULL,
  session_id TEXT REFERENCES sessions(id),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(ticket_id, name)
);
CREATE TABLE IF NOT EXISTS attachment_references (
  attachment_id INTEGER NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY(attachment_id, ticket_id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS ticket_search USING fts5(title, question, resolution);
CREATE TRIGGER IF NOT EXISTS tickets_ai AFTER INSERT ON tickets BEGIN
  INSERT INTO ticket_search(rowid, title, question, resolution)
  VALUES (new.id, new.title, new.question, coalesce(new.resolution, ''));
END;
CREATE TRIGGER IF NOT EXISTS tickets_ad AFTER DELETE ON tickets BEGIN
  DELETE FROM ticket_search WHERE rowid = old.id;
END;
CREATE TRIGGER IF NOT EXISTS tickets_au AFTER UPDATE OF title, question, resolution ON tickets BEGIN
  DELETE FROM ticket_search WHERE rowid = old.id;
  INSERT INTO ticket_search(rowid, title, question, resolution)
  VALUES (new.id, new.title, new.question, coalesce(new.resolution, ''));
END;

-- Added by `migrate_database` on every start, once the table already exists.
ALTER TABLE tickets ADD COLUMN amended_at TEXT;

-- --------------------------------------------------------------------------
-- Rows
-- --------------------------------------------------------------------------

INSERT INTO projects(key, created_at)
VALUES ('/Users/operator/Projects/wayfind', '2026-07-01 09:00:00');

INSERT INTO initiatives(id, project_key, name, destination, notes, status, created_at)
VALUES (
  1,
  '/Users/operator/Projects/wayfind',
  'Port the tracker to Rust',
  'A single binary that reads the Bash script''s database and prints the same documents.',
  'Notes across lines.
The second line has a "quoted" phrase and a	tab.
The third line is 日本語.',
  'working',
  '2026-07-01 09:05:00'
);

-- Sessions come before tickets, and none of them holds a ticket yet. That is
-- the order the script writes in: a session exists from the moment it resumes,
-- and only takes a ticket later. It also keeps every foreign key satisfiable at
-- the moment its row is written, since `sessions` and `tickets` point at each
-- other.
INSERT INTO sessions(id, project_key, initiative_id, current_ticket_id,
                     resolved_non_research_count, status, started_at, last_seen_at)
VALUES
  ('session-holding', '/Users/operator/Projects/wayfind', 1, NULL, 0, 'active',
   '2026-07-02 08:00:00', '2026-07-02 12:00:00'),
  ('session-spent', '/Users/operator/Projects/wayfind', 1, NULL, 1, 'active',
   '2026-07-01 10:00:00', '2026-07-02 11:00:00'),
  ('session-closed', '/Users/operator/Projects/wayfind', 1, NULL, 0, 'closed',
   '2026-07-01 09:30:00', '2026-07-01 18:00:00');

-- 1: resolved, and later amended.
-- 2: resolved.
-- 3: claimed right now by session-holding.
-- 4: open, and blocked by 3.
-- 5: excluded.
INSERT INTO tickets(id, initiative_id, title, type, status, question, resolution,
                    created_at, resolved_at, amended_at)
VALUES
  (1, 1, 'Choose the storage backend', 'grilling', 'resolved',
   'Which store keeps the tickets?',
   'SQLite, bundled.
It is what the script already writes, and the file must keep working untouched.',
   '2026-07-01 09:10:00', '2026-07-01 14:00:00', '2026-07-02 09:00:00'),
  (2, 1, 'Survey the "front matter" readers', 'research', 'resolved',
   'What parses the +++ block today?	Anything strict?',
   'Two agents read it, both with a strict TOML parser. The block has to stay parseable.',
   '2026-07-01 09:12:00', '2026-07-01 16:00:00', NULL),
  (3, 1, 'Draw the dependency tree', 'task', 'claimed',
   'How is the lane diagram laid out?', NULL,
   '2026-07-01 09:14:00', NULL, NULL),
  (4, 1, 'Ship the release workflow', 'task', 'open',
   'Which targets does the release build?', NULL,
   '2026-07-01 09:16:00', NULL, NULL),
  (5, 1, 'Rewrite the shell completions', 'prototype', 'excluded',
   'Do the completions need porting too?', NULL,
   '2026-07-01 09:18:00', NULL, NULL);

INSERT INTO ticket_dependencies(ticket_id, blocker_id) VALUES (4, 3), (3, 1);

-- One live claim and one already released, so a reader must filter on
-- `released_at` rather than trusting the row's existence.
INSERT INTO ticket_claims(ticket_id, session_id, claimed_at, released_at)
VALUES
  (3, 'session-holding', '2026-07-02 08:05:00', NULL),
  (1, 'session-spent', '2026-07-01 13:00:00', '2026-07-01 14:00:00');

-- The claim and the session's own record of it are written together.
UPDATE sessions SET current_ticket_id = 3 WHERE id = 'session-holding';

INSERT INTO decisions(id, ticket_id, gist, created_at)
VALUES
  (1, 1, 'SQLite, bundled.', '2026-07-01 14:00:00'),
  (2, 2, 'Two agents read it, both with a strict TOML parser. The block has to stay parseable.',
   '2026-07-01 16:00:00');

INSERT INTO fog_notes(id, initiative_id, note, created_at)
VALUES
  (1, 1, 'Whether the search backend stays inside the same file.', '2026-07-01 10:00:00'),
  (2, 1, 'How a "clear" initiative is reopened.', '2026-07-01 10:05:00');

INSERT INTO scope_exclusions(id, initiative_id, note, created_at)
VALUES (1, 1, 'A network backend. Not this initiative.', '2026-07-01 10:10:00');

INSERT INTO attachments(id, ticket_id, name, description, content, byte_size,
                        session_id, created_at)
VALUES (
  1, 1, 'backend-comparison.md',
  'The table that settled the storage question.',
  '# Backends

| Name | Verdict |
| --- | --- |
| SQLite | Chosen. The script already writes it. |
| Postgres | A server for a single-user tool. |
',
  138,
  'session-spent',
  '2026-07-01 13:30:00'
);

-- Ticket 3 points at the attachment ticket 1 owns.
INSERT INTO attachment_references(attachment_id, ticket_id, created_at)
VALUES (1, 3, '2026-07-02 08:10:00');

-- The insert trigger has been filling this all along; this only proves the
-- fixture leaves it in the state the script would.
INSERT INTO ticket_search(ticket_search) VALUES ('rebuild');
