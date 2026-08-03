//! Full-text search over the FTS5 index the script's triggers keep.
//!
//! The index is not something this port builds. `tickets_ai`, `tickets_ad`, and
//! `tickets_au` write it on every insert, delete, and update, so a ticket
//! created a moment ago is searchable the moment its transaction commits. This
//! adapter only asks the question.
//!
//! The query string is bound, never interpolated. FTS5 owns its own syntax —
//! `near/3`, `title:backend`, `"exact phrase"` — and an operator who writes
//! nonsense should hear FTS5's complaint, not a parser this port invented. A
//! syntax complaint comes back as [`SearchError::InvalidQuery`]; anything else
//! is infrastructure.
//!
//! Ranking is FTS5's, but it does not leak. The hits come back in the order the
//! boundary promises, and the raw `bm25` score is put only under the namespaced
//! `fts5.rank` metadata key. Nothing that renders a result list reads it.

use rusqlite::{Connection, OpenFlags};
use wayfind_core::{
    SearchBackend, SearchError, SearchHit, SearchPage, SearchRequest, SearchResult,
    SqliteFts5Settings, TicketId, TicketStatusLabel, FTS5_METADATA_NAMESPACE,
};

/// The FTS5 index, open for reading.
#[derive(Debug)]
pub struct SqliteFts5Search {
    connection: Connection,
    table: String,
}

impl SqliteFts5Search {
    /// Open the index named by the resolved configuration.
    ///
    /// This is a second connection to the same file the store holds. SQLite is
    /// built for that: the store commits, this connection reads what was
    /// committed, and neither blocks the other under write-ahead logging.
    ///
    /// It is opened read-write even though it only ever reads. A write-ahead
    /// database needs its shared-memory file, and a read-only connection cannot
    /// create one — so asking for read-only would refuse to open exactly the
    /// databases the script leaves behind. The flag is not the guard here; the
    /// statement is, and the only statement is a `SELECT`.
    pub fn open(settings: &SqliteFts5Settings) -> SearchResult<Self> {
        let table = table_name(&settings.table)?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI;
        let connection =
            Connection::open_with_flags(&settings.database, flags).map_err(|error| {
                SearchError::infrastructure(
                    "open search index",
                    format!("cannot open {}: {error}", settings.database.display()),
                )
            })?;
        Ok(Self { connection, table })
    }

    /// Use a connection that is already open.
    ///
    /// Only tests and callers that have their own reason to share a handle need
    /// this. Ordinary use goes through [`SqliteFts5Search::open`].
    pub fn with_connection(connection: Connection, table: &str) -> SearchResult<Self> {
        Ok(Self {
            connection,
            table: table_name(table)?,
        })
    }

    /// The statement for one page, with the index name written in.
    ///
    /// The table name cannot be bound — SQLite binds values, not identifiers —
    /// so it is checked against [`table_name`] first and written in afterwards.
    /// Everything an operator typed is bound.
    fn page_query(&self) -> String {
        let table = &self.table;
        format!(
            "SELECT t.id AS id, t.title AS title, t.status AS status, \
                    snippet({table}, 1, '**', '**', '…', 12) AS snippet, \
                    bm25({table}) AS rank \
             FROM {table} \
             JOIN tickets t ON t.id = {table}.rowid \
             WHERE {table} MATCH ?1 AND t.initiative_id = ?2 \
             ORDER BY bm25({table}), t.id \
             LIMIT ?3 OFFSET ?4;"
        )
    }
}

/// Accept an index name that is safe to write into a statement.
///
/// Only a plain SQLite identifier is allowed: a letter or underscore, then
/// letters, digits, or underscores. That is what every FTS5 table Wayfind has
/// ever made is called, and refusing anything else means a configuration file
/// can name a table but can never smuggle in SQL.
fn table_name(raw: &str) -> SearchResult<String> {
    let mut characters = raw.chars();
    let acceptable = match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {
            characters.all(|next| next.is_ascii_alphanumeric() || next == '_')
        }
        _ => false,
    };
    if !acceptable {
        return Err(SearchError::infrastructure(
            "open search index",
            format!(
                "`{raw}` is not a usable table name; use letters, digits, and underscores only"
            ),
        ));
    }
    Ok(raw.to_owned())
}

/// Whether a SQLite complaint is about what the operator typed.
///
/// FTS5 reports a bad query through the ordinary error channel, so the text is
/// the only thing that separates "you typed `AND AND`" from "the disk is gone".
/// Reading it is unpleasant but it is where the distinction lives, and getting
/// it wrong in the safe direction only costs a less precise message.
fn is_query_complaint(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("fts5:")
        || lowered.contains("malformed match")
        || lowered.contains("no such column")
        || lowered.contains("unknown special query")
        || lowered.contains("unable to use function match")
}

/// Turn a SQLite failure into the right kind of search error.
fn classify(operation: &'static str) -> impl Fn(rusqlite::Error) -> SearchError {
    move |error| {
        let text = error.to_string();
        if is_query_complaint(&text) {
            return SearchError::invalid_query(text);
        }
        SearchError::infrastructure(operation, text)
    }
}

/// One row of the page query, still in SQLite's vocabulary.
struct Row {
    id: i64,
    title: String,
    status: String,
    snippet: String,
    rank: f64,
}

impl Row {
    /// Turn the row into a hit, or say the stored ticket cannot exist.
    fn into_hit(self) -> SearchResult<SearchHit> {
        let ticket_id = TicketId::new(self.id).map_err(SearchError::CorruptData)?;
        let status = self
            .status
            .parse::<TicketStatusLabel>()
            .map_err(SearchError::CorruptData)?;
        let mut metadata = std::collections::BTreeMap::new();
        // Only if the number survives the trip. A score that is not finite has
        // no JSON spelling, and a missing score is better than a wrong one.
        if let Some(rank) = serde_json::Number::from_f64(self.rank) {
            metadata.insert(
                format!("{FTS5_METADATA_NAMESPACE}.rank"),
                serde_json::Value::Number(rank),
            );
        }
        Ok(SearchHit {
            ticket_id,
            title: self.title,
            status,
            snippet: self.snippet,
            metadata,
        })
    }
}

impl SearchBackend for SqliteFts5Search {
    fn search(&self, request: &SearchRequest) -> SearchResult<SearchPage> {
        // One more row than the caller asked for. If it comes back there is
        // another page; it is then dropped. That is one query instead of two,
        // and it cannot disagree with itself the way a separate count could.
        let probe = i64::from(request.limit.get()) + 1;
        let mut statement = self
            .connection
            .prepare(&self.page_query())
            .map_err(classify("prepare search"))?;
        let rows = statement
            .query_map(
                rusqlite::params![
                    request.query.as_str(),
                    request.initiative_id.get(),
                    probe,
                    i64::from(request.offset.get()),
                ],
                |row| {
                    Ok(Row {
                        id: row.get("id")?,
                        title: row.get("title")?,
                        status: row.get("status")?,
                        snippet: row.get("snippet")?,
                        rank: row.get("rank")?,
                    })
                },
            )
            .map_err(classify("run search"))?
            .collect::<rusqlite::Result<Vec<Row>>>()
            .map_err(classify("read search results"))?;

        let has_more = rows.len() > usize::try_from(request.limit.get()).unwrap_or(usize::MAX);
        let hits = rows
            .into_iter()
            .take(usize::try_from(request.limit.get()).unwrap_or(usize::MAX))
            .map(Row::into_hit)
            .collect::<SearchResult<Vec<SearchHit>>>()?;

        Ok(SearchPage {
            hits,
            request: request.clone(),
            has_more,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{is_query_complaint, table_name};

    #[test]
    fn a_plain_identifier_is_accepted() {
        assert_eq!(table_name("ticket_search").unwrap(), "ticket_search");
        assert_eq!(table_name("_idx9").unwrap(), "_idx9");
    }

    #[test]
    fn anything_that_could_carry_sql_is_refused() {
        for raw in [
            "",
            "9lives",
            "ticket search",
            "ticket_search; DROP TABLE tickets",
            "\"quoted\"",
        ] {
            assert!(
                table_name(raw).is_err(),
                "`{raw}` should not be usable as a table name"
            );
        }
    }

    #[test]
    fn a_syntax_complaint_is_told_apart_from_a_broken_disk() {
        assert!(is_query_complaint("fts5: syntax error near \"AND\""));
        assert!(is_query_complaint("no such column: nickname"));
        assert!(!is_query_complaint("disk I/O error"));
        assert!(!is_query_complaint("database is locked"));
    }
}
