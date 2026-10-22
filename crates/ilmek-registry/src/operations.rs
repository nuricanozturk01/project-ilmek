use ilmek_protocol::Uuid7;
use ilmek_store::Unsigned;
use ilmek_store::rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::RegistryError;

/// Applies a mutation exactly once per `(principal, message type, operation id)`.
///
/// The stored result is written in the caller's transaction, so a crash cannot leave a change
/// without its dedupe record or the reverse (PROTOCOL.md §12).
pub(crate) fn run_operation<Req, Res>(
    transaction: &Transaction<'_>,
    now_ms: u64,
    principal: Uuid7,
    message_type: &str,
    operation_id: Uuid7,
    request: &Req,
    compute: impl FnOnce(&Transaction<'_>) -> Result<Res, RegistryError>,
) -> Result<Res, RegistryError>
where
    Req: Serialize,
    Res: Serialize + DeserializeOwned,
{
    let mut canonical = serde_json::to_value(request)
        .map_err(|_| RegistryError::Internal("request serialization".into()))?;
    if let Value::Object(fields) = &mut canonical {
        fields.remove("operation_id");
    }

    let stored: Option<(String, String)> = transaction
        .query_row(
            "SELECT request_json, result_json FROM operations
             WHERE principal_id = ?1 AND message_type = ?2 AND operation_id = ?3",
            params![
                principal.to_string(),
                message_type,
                operation_id.to_string()
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((stored_request, stored_result)) = stored {
        let same_request = serde_json::from_str::<Value>(&stored_request)
            .is_ok_and(|previous| previous == canonical);
        if !same_request {
            return Err(RegistryError::OperationConflict);
        }
        return serde_json::from_str(&stored_result)
            .map_err(|_| RegistryError::Internal("stored result".into()));
    }

    let result = compute(transaction)?;
    let result_json = serde_json::to_string(&result)
        .map_err(|_| RegistryError::Internal("result serialization".into()))?;
    transaction.execute(
        "INSERT INTO operations
             (principal_id, message_type, operation_id, request_json, result_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            principal.to_string(),
            message_type,
            operation_id.to_string(),
            canonical.to_string(),
            result_json,
            Unsigned(now_ms)
        ],
    )?;
    Ok(result)
}
