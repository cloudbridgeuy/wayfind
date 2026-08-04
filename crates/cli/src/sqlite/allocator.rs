//! Handing out identifiers for the coordination domain's rowid-identified
//! entities.

use rusqlite::{Transaction, TransactionBehavior};
use wayfind_core::id::{InitiativeId, NoteId, QuestionId, RunAttachmentId, RunId, TicketId};
use wayfind_core::storage::coordination::MutableIdAllocator;
use wayfind_core::storage::values::{AllocatedId, IdScope, StorageError, StorageResult};

use super::graph_write::{failed, next_id, SqliteGraph};

impl MutableIdAllocator for SqliteGraph<'_> {
    fn allocate(&self, scope: IdScope) -> StorageResult<AllocatedId> {
        let tx = Transaction::new_unchecked(self.connection, TransactionBehavior::Immediate)
            .map_err(failed("begin allocate"))?;
        let next = next_id(&tx, scope.as_str())?;
        tx.commit().map_err(failed("commit allocate"))?;
        to_allocated_id(scope, next)
    }
}

fn to_allocated_id(scope: IdScope, next: i64) -> StorageResult<AllocatedId> {
    let corrupt = || {
        StorageError::CorruptData(format!(
            "id_sequences produced {next} for scope {}",
            scope.as_str()
        ))
    };
    match scope {
        IdScope::Ticket => TicketId::new(next)
            .map(AllocatedId::Ticket)
            .map_err(|_| corrupt()),
        IdScope::Run => RunId::new(next)
            .map(AllocatedId::Run)
            .map_err(|_| corrupt()),
        IdScope::Question => QuestionId::new(next)
            .map(AllocatedId::Question)
            .map_err(|_| corrupt()),
        IdScope::Attachment => RunAttachmentId::new(next)
            .map(AllocatedId::Attachment)
            .map_err(|_| corrupt()),
        IdScope::Note => NoteId::new(next)
            .map(AllocatedId::Note)
            .map_err(|_| corrupt()),
        IdScope::Initiative => InitiativeId::new(next)
            .map(AllocatedId::Initiative)
            .map_err(|_| corrupt()),
    }
}
