//! Reading the immutable graph back.

use rusqlite::{params, OptionalExtension};
use wayfind_core::id::{Hash, InitiativeId, ProjectKey, RecordId, RecordKind, SnapshotOrdinal};
use wayfind_core::record::{Initiative, Snapshot};
use wayfind_core::storage::graph::GraphReader;
use wayfind_core::storage::values::{StorageError, StorageResult};
use wayfind_core::time::Timestamp;

use super::graph_write::{failed, SqliteGraph};

impl GraphReader for SqliteGraph<'_> {
    fn initiative(&self, id: InitiativeId) -> StorageResult<Option<Initiative>> {
        self.connection
            .query_row(
                "SELECT project_key, name, destination, notes, created_at
                 FROM initiatives WHERE id = ?1",
                [id.get()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(failed("read initiative"))?
            .map(|(project, name, destination, notes, created_at)| {
                Ok(Initiative {
                    id,
                    project: parse_project(&project)?,
                    name,
                    destination,
                    notes,
                    created_at: parse_timestamp(&created_at)?,
                })
            })
            .transpose()
    }

    fn initiatives(&self, project: &ProjectKey) -> StorageResult<Vec<Initiative>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, destination, notes, created_at
                 FROM initiatives WHERE project_key = ?1 ORDER BY id",
            )
            .map_err(failed("read initiatives"))?;
        let rows = statement
            .query_map(params![project.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(failed("read initiatives"))?;

        rows.map(|row| row.map_err(failed("read initiatives")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(|(id, name, destination, notes, created_at)| {
                Ok(Initiative {
                    id: to_initiative_id(id)?,
                    project: project.clone(),
                    name,
                    destination,
                    notes,
                    created_at: parse_timestamp(&created_at)?,
                })
            })
            .collect()
    }

    fn snapshots(&self, id: InitiativeId) -> StorageResult<Vec<Snapshot>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT ordinal, transition_hash, declared_base_ordinal, chain_hash, created_at
                 FROM snapshots WHERE initiative_id = ?1 ORDER BY ordinal",
            )
            .map_err(failed("read snapshots"))?;
        let rows = statement
            .query_map([id.get()], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<u32>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(failed("read snapshots"))?;

        rows.map(|row| row.map_err(failed("read snapshots")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(
                |(ordinal, transition_hash, declared_base, chain_hash, created_at)| {
                    Ok(Snapshot {
                        initiative: id,
                        ordinal: parse_ordinal(ordinal)?,
                        transition: transition_hash
                            .map(|hex| parse_record_id(RecordKind::Transition, &hex))
                            .transpose()?,
                        declared_base: declared_base.map(parse_ordinal).transpose()?,
                        chain_hash: parse_hash(&chain_hash)?,
                        created_at: parse_timestamp(&created_at)?,
                    })
                },
            )
            .collect()
    }

    fn root_members(&self, id: InitiativeId) -> StorageResult<Vec<RecordId>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT record_kind, record_hash FROM root_members
                 WHERE initiative_id = ?1 ORDER BY record_kind, record_hash",
            )
            .map_err(failed("read root members"))?;
        let rows = statement
            .query_map([id.get()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(failed("read root members"))?;

        rows.map(|row| row.map_err(failed("read root members")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(|(kind, hash)| parse_record_id(parse_record_kind(&kind)?, &hash))
            .collect()
    }
}

fn to_initiative_id(raw: i64) -> StorageResult<InitiativeId> {
    InitiativeId::new(raw)
        .map_err(|_| StorageError::CorruptData(format!("initiatives.id {raw} is not positive")))
}

fn parse_project(text: &str) -> StorageResult<ProjectKey> {
    ProjectKey::new(text)
        .map_err(|_| StorageError::CorruptData(format!("{text:?} is not a project key")))
}

fn parse_timestamp(text: &str) -> StorageResult<Timestamp> {
    Timestamp::parse_rfc3339(text)
        .map_err(|_| StorageError::CorruptData(format!("{text:?} is not an RFC3339 timestamp")))
}

fn parse_ordinal(raw: u32) -> StorageResult<SnapshotOrdinal> {
    SnapshotOrdinal::new(raw)
        .map_err(|_| StorageError::CorruptData(format!("{raw} is not a snapshot ordinal")))
}

fn parse_hash(hex: &str) -> StorageResult<Hash> {
    Hash::parse_hex(hex).map_err(|_| StorageError::CorruptData(format!("{hex:?} is not a hash")))
}

fn parse_record_id(kind: RecordKind, hex: &str) -> StorageResult<RecordId> {
    Ok(RecordId::new(kind, parse_hash(hex)?))
}

fn parse_record_kind(word: &str) -> StorageResult<RecordKind> {
    match word {
        "node" => Ok(RecordKind::Node),
        "transition" => Ok(RecordKind::Transition),
        "connection" => Ok(RecordKind::Connection),
        "artifact" => Ok(RecordKind::Artifact),
        other => Err(StorageError::CorruptData(format!(
            "{other:?} is not a record kind"
        ))),
    }
}
