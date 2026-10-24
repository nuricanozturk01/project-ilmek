//! Per-application key-value storage, scoped by community, device identity and application
//! (REQUIREMENTS §18.3). Applications cannot name any other scope.

use std::path::Path;

use ilmek_store::rusqlite::{OptionalExtension, params};
use ilmek_store::{Database, Migration, StoreError};
use serde_json::Value;

use crate::bridge::{Failure, MAX_STORAGE_VALUE_BYTES};

const TOTAL_QUOTA_BYTES: i64 = 5 * 1024 * 1024;

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "kv",
    sql: "
CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE cursors (topic TEXT PRIMARY KEY, cursor TEXT NOT NULL) STRICT;
",
}];

#[derive(Debug, Clone)]
pub(crate) struct AppStorage {
    db: Database,
}

fn failure(error: &StoreError) -> Failure {
    tracing::error!(%error, "application storage failed");
    Failure {
        retryable: true,
        ..Failure::plain("INTERNAL", "local storage failed")
    }
}

impl AppStorage {
    pub(crate) fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                StoreError::Sqlite(ilmek_store::rusqlite::Error::ToSqlConversionFailure(
                    Box::new(e),
                ))
            })?;
        }
        Ok(Self {
            db: Database::open(path, MIGRATIONS)?,
        })
    }

    pub(crate) async fn get(&self, key: String) -> Result<Value, Failure> {
        let stored: Option<String> = self
            .db
            .call(move |c| {
                c.query_row("SELECT value FROM kv WHERE key = ?1", [&key], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(StoreError::from)
            })
            .await
            .map_err(|e| failure(&e))?;
        Ok(stored
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null))
    }

    pub(crate) async fn set(&self, key: String, value: Value) -> Result<(), Failure> {
        let text = serde_json::to_string(&value).map_err(|_| Failure::invalid("value"))?;
        if text.len() > MAX_STORAGE_VALUE_BYTES {
            return Err(Failure::plain("TOO_LARGE", "value is too large"));
        }
        let outcome = self
            .db
            .call(move |c| {
                let tx = c.transaction_with_behavior(
                    ilmek_store::rusqlite::TransactionBehavior::Immediate,
                )?;
                let others: i64 = tx.query_row(
                    "SELECT COALESCE(SUM(LENGTH(key) + LENGTH(value)), 0) FROM kv WHERE key != ?1",
                    [&key],
                    |row| row.get(0),
                )?;
                let size = i64::try_from(key.len() + text.len()).unwrap_or(i64::MAX);
                if others.saturating_add(size) > TOTAL_QUOTA_BYTES {
                    return Ok::<_, StoreError>(false);
                }
                tx.execute(
                    "INSERT INTO kv (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, text],
                )?;
                tx.commit()?;
                Ok(true)
            })
            .await
            .map_err(|e| failure(&e))?;
        if outcome {
            Ok(())
        } else {
            Err(Failure::plain(
                "RESOURCE_EXHAUSTED",
                "storage quota exceeded",
            ))
        }
    }

    pub(crate) async fn remove(&self, key: String) -> Result<(), Failure> {
        self.db
            .call(move |c| {
                c.execute("DELETE FROM kv WHERE key = ?1", [&key])?;
                Ok::<_, StoreError>(())
            })
            .await
            .map_err(|e| failure(&e))
    }

    pub(crate) async fn cursor(&self, topic: String) -> Result<Option<String>, Failure> {
        self.db
            .call(move |c| {
                c.query_row(
                    "SELECT cursor FROM cursors WHERE topic = ?1",
                    [&topic],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::from)
            })
            .await
            .map_err(|e| failure(&e))
    }

    pub(crate) async fn save_cursor(&self, topic: String, cursor: String) -> Result<(), Failure> {
        self.db
            .call(move |c| {
                c.execute(
                    "INSERT INTO cursors (topic, cursor) VALUES (?1, ?2)
                     ON CONFLICT(topic) DO UPDATE SET cursor = excluded.cursor",
                    params![topic, cursor],
                )
                .map_err(StoreError::from)?;
                Ok::<_, StoreError>(())
            })
            .await
            .map_err(|e| failure(&e))
    }
}
