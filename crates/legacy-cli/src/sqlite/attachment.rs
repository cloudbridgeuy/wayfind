//! Storing attachment documents and the pointers to them.
//!
//! Bytes cross the trait boundary opaque, and they are stored the way the script
//! stored them: as SQLite text, so `wayfind attach show` from either program
//! prints the same document. Bytes that are not valid UTF-8 are written as a
//! blob instead. SQLite holds either in the same column without complaint, and a
//! read hands back exactly what was written — which is the promise that matters,
//! because a document is returned to an operator byte for byte or not at all.

use rusqlite::{Connection, OptionalExtension};
use wayfind_v1_core::{
    AddAttachmentReference, AttachmentId, AttachmentMetadata, AttachmentStore, CapacityLimit,
    ReferenceConflict, ReferenceOutcome, RemoveAttachmentOutcome, RemoveAttachmentReference,
    SessionId, StorageError, StorageResult, StoreAttachment, TicketId, MAX_BYTES_PER_WORKFLOW,
};

use super::write::initiative_of_ticket;
use super::{advance_revision, failed, row, SqliteStorage};

const METADATA_SELECT: &str = "\
SELECT id AS id, ticket_id AS ticket_id, name AS name, description AS description,
       byte_size AS byte_size, session_id AS session_id, created_at AS created_at
FROM attachments";

/// The ticket that owns an attachment, if the attachment is there at all.
fn owner(
    connection: &Connection,
    operation: &'static str,
    id: AttachmentId,
) -> StorageResult<Option<TicketId>> {
    connection
        .query_row(
            "SELECT ticket_id FROM attachments WHERE id = ?1;",
            [id.get()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(failed(operation))?
        .map(|raw| TicketId::new(raw).map_err(StorageError::CorruptData))
        .transpose()
}

/// Move the revision of the initiative a ticket sits in.
///
/// A document appears in `attach list`, which is an initiative-scoped read, so
/// storing, removing, or re-pointing one moves the initiative on. A ticket with
/// no initiative cannot happen — the column is `NOT NULL` — but a store never
/// asserts that; it simply has nothing to move.
fn advance_owning_initiative(
    connection: &Connection,
    operation: &'static str,
    ticket_id: TicketId,
) -> StorageResult<()> {
    if let Some(initiative_id) = initiative_of_ticket(connection, operation, ticket_id)? {
        advance_revision(connection, initiative_id)?;
    }
    Ok(())
}

impl AttachmentStore for SqliteStorage {
    fn store_attachment(
        &self,
        command: StoreAttachment,
        bytes: &[u8],
    ) -> StorageResult<AttachmentMetadata> {
        // A backstop, not the policy. The shell applies the script's own smaller
        // cap before it ever reads a file; this only refuses what no backend
        // could commit in one go.
        if bytes.len() > MAX_BYTES_PER_WORKFLOW {
            return Err(StorageError::CapacityExceeded(
                CapacityLimit::ValueTooLarge {
                    field: "attachment content",
                    limit: MAX_BYTES_PER_WORKFLOW,
                    actual: bytes.len(),
                },
            ));
        }

        let transaction = self.write_transaction()?;
        let byte_size = i64::try_from(bytes.len()).map_err(|_| {
            StorageError::infrastructure("store attachment", "the document is too large to record")
        })?;
        // Text when the document is text, which is nearly always, and which is
        // what keeps the file readable from the script and from `sqlite3`.
        let content: rusqlite::types::Value = match std::str::from_utf8(bytes) {
            Ok(text) => rusqlite::types::Value::Text(text.to_owned()),
            Err(_) => rusqlite::types::Value::Blob(bytes.to_vec()),
        };
        transaction
            .execute(
                "INSERT INTO attachments(id, ticket_id, name, description, content, byte_size, \
                                         session_id, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
                rusqlite::params![
                    command.id.get(),
                    command.ticket_id.get(),
                    command.name.as_str(),
                    command.description,
                    content,
                    byte_size,
                    command.session_id.as_ref().map(SessionId::as_str),
                    command.now.to_storage_string(),
                ],
            )
            .map_err(|error| {
                let text = error.to_string();
                if text.contains("UNIQUE constraint failed") {
                    return StorageError::infrastructure(
                        "store attachment",
                        "this ticket already has an attachment with that name",
                    );
                }
                StorageError::infrastructure("store attachment", text)
            })?;
        advance_owning_initiative(&transaction, "store attachment", command.ticket_id)?;
        let stored = transaction
            .query_row(
                &format!("{METADATA_SELECT} WHERE id = ?1;"),
                [command.id.get()],
                |row| Ok(row::parse_attachment_metadata(row)),
            )
            .map_err(failed("store attachment"))??;
        transaction.commit().map_err(failed("store attachment"))?;
        Ok(stored)
    }

    fn read_attachment(&self, id: AttachmentId) -> StorageResult<Option<Vec<u8>>> {
        self.connection()
            .query_row(
                "SELECT content FROM attachments WHERE id = ?1;",
                [id.get()],
                |row| {
                    row.get_ref(0)
                        .and_then(|value| Ok(value.as_bytes()?.to_vec()))
                },
            )
            .optional()
            .map_err(failed("read attachment"))
    }

    fn remove_attachment(&self, id: AttachmentId) -> StorageResult<RemoveAttachmentOutcome> {
        let transaction = self.write_transaction()?;
        let Some(ticket_id) = owner(&transaction, "remove attachment", id)? else {
            return Ok(RemoveAttachmentOutcome::NotFound);
        };

        // Counted before the delete, because `ON DELETE CASCADE` takes the
        // reference rows with it and there is nothing left to count after.
        let references: i64 = transaction
            .query_row(
                "SELECT count(*) FROM attachment_references WHERE attachment_id = ?1;",
                [id.get()],
                |row| row.get(0),
            )
            .map_err(failed("remove attachment"))?;
        transaction
            .execute("DELETE FROM attachments WHERE id = ?1;", [id.get()])
            .map_err(failed("remove attachment"))?;
        advance_owning_initiative(&transaction, "remove attachment", ticket_id)?;
        transaction.commit().map_err(failed("remove attachment"))?;
        Ok(RemoveAttachmentOutcome::Removed {
            references_removed: u32::try_from(references).unwrap_or(u32::MAX),
        })
    }

    fn add_reference(&self, command: AddAttachmentReference) -> StorageResult<ReferenceOutcome> {
        let transaction = self.write_transaction()?;
        let Some(owning_ticket) = owner(&transaction, "add reference", command.attachment_id)?
        else {
            return Ok(ReferenceOutcome::Conflict(
                ReferenceConflict::NoSuchAttachment,
            ));
        };
        if owning_ticket == command.ticket_id {
            // A ticket's own documents are already listed under it. A reference
            // would say the same thing twice.
            return Ok(ReferenceOutcome::Conflict(
                ReferenceConflict::TicketOwnsAttachment {
                    ticket_id: command.ticket_id,
                },
            ));
        }
        if initiative_of_ticket(&transaction, "add reference", command.ticket_id)?.is_none() {
            return Ok(ReferenceOutcome::Conflict(
                ReferenceConflict::NoSuchTicket {
                    ticket_id: command.ticket_id,
                },
            ));
        }

        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO attachment_references(attachment_id, ticket_id, \
                                                             created_at) \
                 VALUES (?1, ?2, ?3);",
                rusqlite::params![
                    command.attachment_id.get(),
                    command.ticket_id.get(),
                    command.now.to_storage_string(),
                ],
            )
            .map_err(failed("add reference"))?;
        if inserted == 0 {
            return Ok(ReferenceOutcome::AlreadyPresent);
        }
        advance_owning_initiative(&transaction, "add reference", command.ticket_id)?;
        transaction.commit().map_err(failed("add reference"))?;
        Ok(ReferenceOutcome::Added)
    }

    fn remove_reference(
        &self,
        command: RemoveAttachmentReference,
    ) -> StorageResult<ReferenceOutcome> {
        let transaction = self.write_transaction()?;
        let removed = transaction
            .execute(
                "DELETE FROM attachment_references WHERE attachment_id = ?1 AND ticket_id = ?2;",
                rusqlite::params![command.attachment_id.get(), command.ticket_id.get()],
            )
            .map_err(failed("remove reference"))?;
        if removed == 0 {
            // Nothing to drop is not a refusal. Asking twice is allowed, and a
            // reference that is already gone is the state the caller wanted.
            return Ok(ReferenceOutcome::NotPresent);
        }
        advance_owning_initiative(&transaction, "remove reference", command.ticket_id)?;
        transaction.commit().map_err(failed("remove reference"))?;
        Ok(ReferenceOutcome::Removed)
    }
}
