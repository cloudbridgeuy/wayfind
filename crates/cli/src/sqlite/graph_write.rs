//! T2: writing a new initiative's first record.
//!
//! One `IMMEDIATE` transaction, no file I/O, and no hash computed here — every
//! hash [`ValidatedInitiative`] carries was computed by the core before this
//! adapter ever saw it. A duplicate `(project_key, name)` is read back as
//! [`CreateInitiativeOutcome::NameTaken`] before anything is written, because a
//! conflict is a value, not an error.

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use wayfind_core::id::{InitiativeId, RecordKind};
use wayfind_core::outcome::graph::CreateInitiativeOutcome;
use wayfind_core::record::Initiative;
use wayfind_core::storage::graph::GraphAppender;
use wayfind_core::storage::values::{StorageError, StorageResult};
use wayfind_core::validate::initiative::ValidatedInitiative;

/// The immutable graph, over one connection.
pub struct SqliteGraph<'a> {
    connection: &'a Connection,
}

impl<'a> SqliteGraph<'a> {
    /// Wrap a connection that already holds the v2 schema.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
}

impl GraphAppender for SqliteGraph<'_> {
    fn create_initiative(
        &self,
        validated: ValidatedInitiative,
    ) -> StorageResult<CreateInitiativeOutcome> {
        let tx = Transaction::new_unchecked(self.connection, TransactionBehavior::Immediate)
            .map_err(failed("begin create_initiative"))?;

        if let Some(existing) = existing_initiative(&tx, &validated)? {
            return Ok(CreateInitiativeOutcome::NameTaken { existing });
        }

        let id = allocate_initiative_id(&tx)?;
        insert_project(&tx, &validated)?;
        insert_initiative(&tx, id, &validated)?;
        insert_destination_node(&tx, &validated)?;
        insert_root_snapshot(&tx, id, &validated)?;
        insert_root_member(&tx, id, &validated)?;

        tx.commit().map_err(failed("commit create_initiative"))?;

        Ok(CreateInitiativeOutcome::Created(Initiative {
            id,
            project: validated.project,
            name: validated.name,
            destination: validated.destination,
            notes: validated.notes,
            created_at: validated.snapshot.created_at,
        }))
    }
}

/// Turn a SQLite failure into an infrastructure error naming the operation.
fn failed(operation: &'static str) -> impl Fn(rusqlite::Error) -> StorageError {
    move |error| StorageError::infrastructure(operation, error.to_string())
}

/// A row already in the database is corrupt data, not a fault an operator
/// caused: nothing this adapter writes can produce a non-positive identifier.
fn to_initiative_id(raw: i64) -> StorageResult<InitiativeId> {
    InitiativeId::new(raw)
        .map_err(|_| StorageError::CorruptData(format!("initiatives.id {raw} is not positive")))
}

fn existing_initiative(
    tx: &Transaction,
    validated: &ValidatedInitiative,
) -> StorageResult<Option<InitiativeId>> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM initiatives WHERE project_key = ?1 AND name = ?2",
            params![validated.project.as_str(), validated.name],
            |row| row.get(0),
        )
        .optional()
        .map_err(failed("create_initiative"))?;
    existing.map(to_initiative_id).transpose()
}

/// Draw the next identifier from the `initiative` scope.
///
/// This slice's store is always freshly created, so unlike v1's migrated
/// databases there are no un-counted rows to fall back to: the counter is the
/// only source of truth.
fn allocate_initiative_id(tx: &Transaction) -> StorageResult<InitiativeId> {
    let current: Option<i64> = tx
        .query_row(
            "SELECT next_id FROM id_sequences WHERE scope = 'initiative'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(failed("allocate initiative id"))?;
    let next = current.unwrap_or(0) + 1;
    tx.execute(
        "INSERT INTO id_sequences (scope, next_id) VALUES ('initiative', ?1)
         ON CONFLICT(scope) DO UPDATE SET next_id = excluded.next_id",
        [next],
    )
    .map_err(failed("allocate initiative id"))?;
    to_initiative_id(next)
}

fn insert_project(tx: &Transaction, validated: &ValidatedInitiative) -> StorageResult<()> {
    tx.execute(
        "INSERT INTO projects (key, created_at) VALUES (?1, ?2)
         ON CONFLICT(key) DO NOTHING",
        params![
            validated.project.as_str(),
            validated.snapshot.created_at.to_storage_string(),
        ],
    )
    .map_err(failed("insert project"))?;
    Ok(())
}

fn insert_initiative(
    tx: &Transaction,
    id: InitiativeId,
    validated: &ValidatedInitiative,
) -> StorageResult<()> {
    tx.execute(
        "INSERT INTO initiatives (id, project_key, name, destination, notes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id.get(),
            validated.project.as_str(),
            validated.name,
            validated.destination,
            validated.notes,
            validated.snapshot.created_at.to_storage_string(),
        ],
    )
    .map_err(failed("insert initiative"))?;
    Ok(())
}

/// Insert the destination node, sharing the row when its hash already names
/// one: the immutable graph is content-addressed, so an identical record is
/// not a second one.
fn insert_destination_node(tx: &Transaction, validated: &ValidatedInitiative) -> StorageResult<()> {
    let node = &validated.destination_node;
    tx.execute(
        "INSERT INTO result_nodes (hash, node_kind, title, summary, content, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(hash) DO NOTHING",
        params![
            node.id.hash().to_hex(),
            node.draft.node_kind.as_token(),
            node.draft.title,
            node.draft.summary.clone().unwrap_or_default(),
            node.draft.content,
            node.draft.created_at.to_storage_string(),
            node.draft.created_by.as_str(),
        ],
    )
    .map_err(failed("insert destination node"))?;
    Ok(())
}

fn insert_root_snapshot(
    tx: &Transaction,
    id: InitiativeId,
    validated: &ValidatedInitiative,
) -> StorageResult<()> {
    tx.execute(
        "INSERT INTO snapshots
             (initiative_id, ordinal, transition_hash, declared_base_ordinal, chain_hash, created_at)
         VALUES (?1, ?2, NULL, NULL, ?3, ?4)",
        params![
            id.get(),
            validated.snapshot.ordinal.get(),
            validated.snapshot.chain_hash.to_hex(),
            validated.snapshot.created_at.to_storage_string(),
        ],
    )
    .map_err(failed("insert root snapshot"))?;
    Ok(())
}

fn insert_root_member(
    tx: &Transaction,
    id: InitiativeId,
    validated: &ValidatedInitiative,
) -> StorageResult<()> {
    tx.execute(
        "INSERT INTO root_members (initiative_id, record_kind, record_hash) VALUES (?1, ?2, ?3)",
        params![
            id.get(),
            RecordKind::Node.as_encoding_word(),
            validated.destination_node.id.hash().to_hex(),
        ],
    )
    .map_err(failed("insert root member"))?;
    Ok(())
}
