//! Reading the immutable graph back.

use std::str::FromStr;

use rusqlite::{params, OptionalExtension};
use wayfind_core::id::{
    Hash, InitiativeId, ProjectKey, RecordId, RecordKind, SessionId, SnapshotOrdinal,
};
use wayfind_core::kinds::{NodeKind, TransitionKind};
use wayfind_core::record::{
    ImportDraft, Initiative, NodeDraft, ResultNode, Snapshot, Transition, TransitionDraft,
};
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

    fn snapshot(
        &self,
        id: InitiativeId,
        ordinal: SnapshotOrdinal,
    ) -> StorageResult<Option<Snapshot>> {
        self.connection
            .query_row(
                "SELECT transition_hash, declared_base_ordinal, chain_hash, created_at
                 FROM snapshots WHERE initiative_id = ?1 AND ordinal = ?2",
                params![id.get(), ordinal.get()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<u32>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(failed("read snapshot"))?
            .map(|(transition_hash, declared_base, chain_hash, created_at)| {
                Ok(Snapshot {
                    initiative: id,
                    ordinal,
                    transition: transition_hash
                        .map(|hex| parse_record_id(RecordKind::Transition, &hex))
                        .transpose()?,
                    declared_base: declared_base.map(parse_ordinal).transpose()?,
                    chain_hash: parse_hash(&chain_hash)?,
                    created_at: parse_timestamp(&created_at)?,
                })
            })
            .transpose()
    }

    fn accepted_transitions(
        &self,
        id: InitiativeId,
        through: SnapshotOrdinal,
    ) -> StorageResult<Vec<Transition>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT t.hash, t.transition_kind, t.summary, t.rationale, t.created_at, t.created_by
                 FROM snapshots s JOIN transitions t ON t.hash = s.transition_hash
                 WHERE s.initiative_id = ?1 AND s.ordinal BETWEEN 2 AND ?2
                 ORDER BY s.ordinal",
            )
            .map_err(failed("read accepted transitions"))?;
        let rows = statement
            .query_map(params![id.get(), through.get()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(failed("read accepted transitions"))?;

        rows.map(|row| row.map_err(failed("read accepted transitions")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(|(hash, kind, summary, rationale, created_at, created_by)| {
                Ok(Transition {
                    id: parse_record_id(RecordKind::Transition, &hash)?,
                    draft: TransitionDraft {
                        transition_kind: parse_transition_kind(&kind)?,
                        summary,
                        rationale,
                        inputs: self.transition_nodes(&hash, "transition_inputs")?,
                        outputs: self.transition_nodes(&hash, "transition_outputs")?,
                        created_at: parse_timestamp(&created_at)?,
                        created_by: parse_session_id(&created_by)?,
                        import: self.read_import(&hash)?,
                    },
                })
            })
            .collect()
    }

    fn node(&self, hash: &Hash) -> StorageResult<Option<ResultNode>> {
        self.connection
            .query_row(
                "SELECT node_kind, title, summary, content, created_at, created_by
                 FROM result_nodes WHERE hash = ?1",
                [hash.to_hex()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(failed("read node"))?
            .map(|(kind, title, summary, content, created_at, created_by)| {
                Ok(ResultNode {
                    id: RecordId::new(RecordKind::Node, *hash),
                    draft: NodeDraft {
                        node_kind: parse_node_kind(&kind)?,
                        title,
                        summary: non_empty(summary),
                        content,
                        created_at: parse_timestamp(&created_at)?,
                        created_by: parse_session_id(&created_by)?,
                    },
                })
            })
            .transpose()
    }

    fn resolve_prefix(&self, kind: RecordKind, hex: &str) -> StorageResult<Vec<RecordId>> {
        let sql = format!(
            "SELECT hash FROM {} WHERE hash LIKE ?1 || '%' ORDER BY hash",
            table_for_kind(kind)
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(failed("resolve prefix"))?;
        let rows = statement
            .query_map([hex], |row| row.get::<_, String>(0))
            .map_err(failed("resolve prefix"))?;

        rows.map(|row| row.map_err(failed("resolve prefix")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(|hex| parse_record_id(kind, &hex))
            .collect()
    }
}

impl SqliteGraph<'_> {
    /// The node ids a transition's `transition_inputs` or `transition_outputs`
    /// row names, in manifest position order.
    fn transition_nodes(
        &self,
        transition_hash: &str,
        table: &'static str,
    ) -> StorageResult<Vec<RecordId>> {
        let sql =
            format!("SELECT node_hash FROM {table} WHERE transition_hash = ?1 ORDER BY position");
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(failed("read transition nodes"))?;
        let rows = statement
            .query_map([transition_hash], |row| row.get::<_, String>(0))
            .map_err(failed("read transition nodes"))?;

        rows.map(|row| row.map_err(failed("read transition nodes")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(|hex| parse_record_id(RecordKind::Node, &hex))
            .collect()
    }

    /// A transition's import provenance and members, when it has any.
    fn read_import(&self, transition_hash: &str) -> StorageResult<Option<ImportDraft>> {
        let provenance: Option<(i64, u32, String)> = self
            .connection
            .query_row(
                "SELECT source_initiative_id, source_snapshot_ordinal, rationale
                 FROM import_provenance WHERE transition_hash = ?1",
                [transition_hash],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(failed("read import provenance"))?;

        let Some((source_initiative, source_ordinal, rationale)) = provenance else {
            return Ok(None);
        };

        let mut statement = self
            .connection
            .prepare(
                "SELECT record_kind, record_hash FROM import_members
                 WHERE transition_hash = ?1 ORDER BY record_kind, record_hash",
            )
            .map_err(failed("read import members"))?;
        let rows = statement
            .query_map([transition_hash], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(failed("read import members"))?;
        let included = rows
            .map(|row| row.map_err(failed("read import members")))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .map(|(kind, hash)| parse_record_id(parse_record_kind(&kind)?, &hash))
            .collect::<StorageResult<Vec<_>>>()?;

        Ok(Some(ImportDraft {
            source_initiative: to_initiative_id(source_initiative)?,
            source_snapshot: parse_ordinal(source_ordinal)?,
            included,
            rationale,
        }))
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

/// The table a record kind's rows live in.
fn table_for_kind(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Node => "result_nodes",
        RecordKind::Transition => "transitions",
        RecordKind::Connection => "connections",
        RecordKind::Artifact => "artifacts",
    }
}

/// Map the stored empty-string-means-absent convention back to `Option`.
fn non_empty(text: String) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn parse_node_kind(word: &str) -> StorageResult<NodeKind> {
    NodeKind::from_str(word)
        .map_err(|_| StorageError::CorruptData(format!("{word:?} is not a node kind")))
}

fn parse_transition_kind(word: &str) -> StorageResult<TransitionKind> {
    TransitionKind::from_str(word)
        .map_err(|_| StorageError::CorruptData(format!("{word:?} is not a transition kind")))
}

fn parse_session_id(text: &str) -> StorageResult<SessionId> {
    SessionId::new(text)
        .map_err(|_| StorageError::CorruptData(format!("{text:?} is not a session id")))
}
