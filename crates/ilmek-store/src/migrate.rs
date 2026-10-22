use rusqlite::{Connection, TransactionBehavior};

use super::StoreError;

/// One forward-only schema step. Rollback plans belong in the migration's SQL comments.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub(super) fn apply(
    connection: &mut Connection,
    migrations: &[Migration],
) -> Result<(), StoreError> {
    let ordered = migrations
        .iter()
        .zip(1..)
        .all(|(m, expected)| m.version == expected);
    if !ordered {
        return Err(StoreError::MigrationOrder);
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             name TEXT NOT NULL
         ) STRICT",
    )?;
    let current: u32 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    let known = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    if current > known {
        return Err(StoreError::SchemaTooNew {
            found: current,
            known,
        });
    }
    for migration in migrations.iter().filter(|m| m.version > current) {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            (migration.version, migration.name),
        )?;
        transaction.commit()?;
    }
    Ok(())
}
