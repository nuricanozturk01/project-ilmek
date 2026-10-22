#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use ilmek_store::{Database, Migration, StoreError, restore};
use tempfile::tempdir;

const V1: Migration = Migration {
    version: 1,
    name: "accounts",
    sql: "CREATE TABLE ledger (id INTEGER PRIMARY KEY, delta INTEGER NOT NULL) STRICT;
          CREATE TABLE totals (id INTEGER PRIMARY KEY CHECK (id = 1), sum INTEGER NOT NULL) STRICT;
          INSERT INTO totals VALUES (1, 0);",
};
const V2: Migration = Migration {
    version: 2,
    name: "notes",
    sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL) STRICT;",
};

fn count(database: &Database, table: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    database
        .call_blocking::<_, StoreError>(|c| Ok(c.query_row(&sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn reopening_does_not_reapply_migrations() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.db");
    drop(Database::open(&path, &[V1]).unwrap());
    let database = Database::open(&path, &[V1, V2]).unwrap();
    let applied: i64 = database
        .call_blocking::<_, StoreError>(|c| {
            Ok(c.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))?)
        })
        .unwrap();
    assert_eq!(applied, 2);
    assert_eq!(count(&database, "totals"), 1);
}

#[test]
fn a_failing_migration_leaves_no_trace() {
    let bad = Migration {
        version: 2,
        name: "broken",
        sql: "CREATE TABLE half (id INTEGER); INSERT INTO nowhere VALUES (1);",
    };
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.db");
    assert!(Database::open(&path, &[V1, bad]).is_err());

    let database = Database::open(&path, &[V1]).unwrap();
    let tables: i64 = database
        .call_blocking::<_, StoreError>(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'half'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(tables, 0);
}

#[test]
fn a_database_from_a_newer_build_is_refused() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.db");
    drop(Database::open(&path, &[V1, V2]).unwrap());
    let error = Database::open(&path, &[V1]).unwrap_err();
    assert!(
        matches!(error, StoreError::SchemaTooNew { found: 2, known: 1 }),
        "{error:?}"
    );
}

#[test]
fn migration_numbering_gaps_are_rejected() {
    let gap = Migration { version: 3, ..V2 };
    assert!(matches!(
        Database::open_in_memory(&[V1, gap]),
        Err(StoreError::MigrationOrder)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn backup_taken_during_writes_is_transactionally_consistent() {
    let dir = tempdir().unwrap();
    let database = Database::open(&dir.path().join("live.db"), &[V1]).unwrap();

    let writer = database.clone();
    let writing = tokio::spawn(async move {
        for n in 1..=400_i64 {
            writer
                .call::<_, StoreError, _>(move |c| {
                    let tx = c.transaction()?;
                    tx.execute("INSERT INTO ledger (delta) VALUES (?1)", [n])?;
                    tx.execute("UPDATE totals SET sum = sum + ?1", [n])?;
                    tx.commit()?;
                    Ok(())
                })
                .await
                .unwrap();
        }
    });
    tokio::task::yield_now().await;
    let backup_path = dir.path().join("backup.db");
    let backing_up = database.clone();
    let target = backup_path.clone();
    tokio::task::spawn_blocking(move || backing_up.backup_to(&target))
        .await
        .unwrap()
        .unwrap();
    writing.await.unwrap();

    let restored_path = dir.path().join("restored.db");
    restore(&backup_path, &restored_path).unwrap();
    let restored = Database::open(&restored_path, &[V1]).unwrap();
    let (ledger_sum, total): (i64, i64) = restored
        .call_blocking::<_, StoreError>(|c| {
            Ok(c.query_row(
                "SELECT COALESCE((SELECT SUM(delta) FROM ledger), 0), (SELECT sum FROM totals)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(ledger_sum, total, "backup split a transaction");
}

#[test]
fn backup_and_restore_refuse_to_overwrite() {
    let dir = tempdir().unwrap();
    let database = Database::open(&dir.path().join("live.db"), &[V1]).unwrap();
    let existing = dir.path().join("exists.db");
    std::fs::write(&existing, b"x").unwrap();

    assert!(matches!(
        database.backup_to(&existing),
        Err(StoreError::DestinationExists(_))
    ));
    assert!(matches!(
        restore(&existing, &existing),
        Err(StoreError::DestinationExists(_))
    ));
}

#[test]
fn restoring_a_corrupt_file_fails() {
    let dir = tempdir().unwrap();
    let junk = dir.path().join("junk.db");
    std::fs::write(&junk, vec![7u8; 4096]).unwrap();
    assert!(restore(&junk, &dir.path().join("out.db")).is_err());
}
