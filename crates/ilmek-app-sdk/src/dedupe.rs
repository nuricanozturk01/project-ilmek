use ilmek_protocol::wire::ErrorCode;
use ilmek_store::Database;
use ilmek_store::rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::backend::CallContext;
use crate::error::AppError;
use crate::events::Events;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ilmek_operations (
    principal_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    method TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    result BLOB NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (principal_id, operation_id)
) STRICT;
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Applied,
    Replayed,
}

/// Exactly-once application of writes per `(principal, operation id)`.
///
/// The business change, its events and the stored result commit in one SQLite transaction, so a
/// crash cannot leave one without the others (PROTOCOL.md §12).
#[derive(Debug, Clone)]
pub struct Operations {
    db: Database,
    events: Events,
}

impl Operations {
    pub(crate) fn new(db: Database, events: Events) -> Self {
        Self { db, events }
    }

    /// Applies `apply` unless this operation already committed, in which case the stored result
    /// is returned. Reusing an operation id for a different method or body is a conflict.
    pub async fn write<F>(
        &self,
        context: &CallContext,
        apply: F,
    ) -> Result<(Vec<u8>, WriteOutcome), AppError>
    where
        F: FnOnce(&Transaction<'_>, &Events) -> Result<Vec<u8>, AppError> + Send + 'static,
    {
        let operation_id = context
            .operation_id
            .ok_or_else(|| AppError::invalid("writes need an operation id"))?;
        let (principal, method, hash) = (
            context.principal_id.to_string(),
            context.method.to_string(),
            context.payload_hash.to_string(),
        );
        let events = self.events.clone();

        let outcome = self
            .db
            .call(move |connection| {
                let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let stored: Option<(String, String, Vec<u8>)> = tx
                    .query_row(
                        "SELECT method, payload_hash, result FROM ilmek_operations
                         WHERE principal_id = ?1 AND operation_id = ?2",
                        params![principal, operation_id.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                if let Some((stored_method, stored_hash, result)) = stored {
                    if stored_method != method || stored_hash != hash {
                        return Err(AppError::new(
                            ErrorCode::OperationConflict,
                            "operation id reused with different content",
                        ));
                    }
                    return Ok((result, WriteOutcome::Replayed));
                }
                let result = apply(&tx, &events).inspect_err(|_| events.rolled_back())?;
                tx.execute(
                    "INSERT INTO ilmek_operations
                         (principal_id, operation_id, method, payload_hash, result, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, CAST(strftime('%s','now') AS INTEGER) * 1000)",
                    params![principal, operation_id.to_string(), method, hash, result],
                )?;
                tx.commit()?;
                events.committed();
                Ok((result, WriteOutcome::Applied))
            })
            .await?;
        Ok(outcome)
    }
}
