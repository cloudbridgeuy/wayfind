//! The FTS5 search index.
//!
//! One table carries every searchable kind, so one BM25 ranks them together and
//! a cross-kind result list merges honestly. `kind` says which corpus a row came
//! from and `ref` says which record: a node's hash, a question's or a ticket's
//! id.
//!
//! FTS5 is the SQLite adapter's answer to the `SearchIndex` capability, never
//! its shape. Nothing above the adapter knows this table is here.

/// The virtual table and the triggers that fill it.
///
/// `result_nodes` is immutable, so it needs an insert trigger and nothing more.
/// `questions` and `tickets` change, so each reindexes on update by deleting its
/// row and writing it again — the v1 approach, kept because an FTS5 external
/// content table would tie the index to a source table this adapter would then
/// have to keep in step by hand.
///
/// Every `ref` is cast to text. FTS5 keeps the type it was given, so an integer
/// id written raw would not match the text the reindex deletes by, and the
/// update would leave the old row behind.
pub const DDL: &str = "\
CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(kind, ref, title, body);

CREATE TRIGGER IF NOT EXISTS search_result_nodes_insert
AFTER INSERT ON result_nodes
BEGIN
  INSERT INTO search_index (kind, ref, title, body)
  VALUES ('node', new.hash, new.title, new.summary || ' ' || new.content);
END;

CREATE TRIGGER IF NOT EXISTS search_questions_insert
AFTER INSERT ON questions
BEGIN
  INSERT INTO search_index (kind, ref, title, body)
  VALUES ('question', cast(new.id as text), new.title,
          new.question || ' ' || coalesce(new.resolution, ''));
END;

CREATE TRIGGER IF NOT EXISTS search_questions_update
AFTER UPDATE ON questions
BEGIN
  DELETE FROM search_index WHERE kind = 'question' AND ref = cast(old.id as text);
  INSERT INTO search_index (kind, ref, title, body)
  VALUES ('question', cast(new.id as text), new.title,
          new.question || ' ' || coalesce(new.resolution, ''));
END;

CREATE TRIGGER IF NOT EXISTS search_tickets_insert
AFTER INSERT ON tickets
BEGIN
  INSERT INTO search_index (kind, ref, title, body)
  VALUES ('ticket', cast(new.id as text), new.title, new.description);
END;

CREATE TRIGGER IF NOT EXISTS search_tickets_update
AFTER UPDATE ON tickets
BEGIN
  DELETE FROM search_index WHERE kind = 'ticket' AND ref = cast(old.id as text);
  INSERT INTO search_index (kind, ref, title, body)
  VALUES ('ticket', cast(new.id as text), new.title, new.description);
END;
";
