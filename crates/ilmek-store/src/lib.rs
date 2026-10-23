//! SQLite with forward-only migrations and consistent backups (REQUIREMENTS §22).

mod backup;
mod migrate;
mod unsigned;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rusqlite::Connection;
use rusqlite::backup::Backup;
use thiserror::Error;

pub use backup::{
    BackupError, BackupReport, MANIFEST_FILE, RestoreReport, backup_tree, restore_tree,
    verify_backup,
};
pub use migrate::Migration;
pub use rusqlite;
pub use unsigned::Unsigned;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const BACKUP_PAGES_PER_STEP: i32 = 256;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database schema version {found} is newer than this build supports ({known})")]
    SchemaTooNew { found: u32, known: u32 },
    #[error("migrations must be numbered 1..n without gaps")]
    MigrationOrder,
    #[error("backup failed integrity check: {0}")]
    Integrity(String),
    #[error("destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("database worker failed")]
    Worker,
}

/// A single SQLite connection behind a mutex; blocking work runs off the async runtime.
#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    pub fn open(path: &Path, migrations: &[Migration]) -> Result<Self, StoreError> {
        Self::prepare(Connection::open(path)?, migrations)
    }

    pub fn open_in_memory(migrations: &[Migration]) -> Result<Self, StoreError> {
        Self::prepare(Connection::open_in_memory()?, migrations)
    }

    fn prepare(mut connection: Connection, migrations: &[Migration]) -> Result<Self, StoreError> {
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migrate::apply(&mut connection, migrations)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Runs `work` on the blocking pool with exclusive access to the connection.
    pub async fn call<T, E, F>(&self, work: F) -> Result<T, E>
    where
        T: Send + 'static,
        E: From<StoreError> + Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, E> + Send + 'static,
    {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.call_blocking(work))
            .await
            .map_err(|_| E::from(StoreError::Worker))?
    }

    pub fn call_blocking<T, E>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        work(&mut connection)
    }

    /// Writes a consistent copy using SQLite's online backup API (safe with WAL).
    pub fn backup_to(&self, destination: &Path) -> Result<(), StoreError> {
        if destination.exists() {
            return Err(StoreError::DestinationExists(destination.to_path_buf()));
        }
        let mut target = Connection::open(destination)?;
        self.call_blocking::<_, StoreError>(|source| {
            let backup = Backup::new(source, &mut target)?;
            backup.run_to_completion(BACKUP_PAGES_PER_STEP, Duration::from_millis(10), None)?;
            Ok(())
        })?;
        finish_copy(&target)
    }
}

/// Copies a live database file to a new path with the online backup API.
pub fn backup_file(source: &Path, destination: &Path) -> Result<(), StoreError> {
    if destination.exists() {
        return Err(StoreError::DestinationExists(destination.to_path_buf()));
    }
    let source = Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let mut target = Connection::open(destination)?;
    let copy = Backup::new(&source, &mut target)?;
    copy.run_to_completion(BACKUP_PAGES_PER_STEP, Duration::from_millis(10), None)?;
    drop(copy);
    finish_copy(&target)
}

/// Restores `backup` into a new database file after verifying it.
pub fn restore(backup: &Path, destination: &Path) -> Result<(), StoreError> {
    if destination.exists() {
        return Err(StoreError::DestinationExists(destination.to_path_buf()));
    }
    let source = Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_integrity(&source)?;
    let mut target = Connection::open(destination)?;
    let copy = Backup::new(&source, &mut target)?;
    copy.run_to_completion(BACKUP_PAGES_PER_STEP, Duration::from_millis(10), None)?;
    drop(copy);
    finish_copy(&target)
}

/// Makes a copy self-contained: the source's WAL mode is carried over by the backup API, which
/// would otherwise leave `-wal`/`-shm` side files next to the copy whenever it is opened.
/// `Database::open` switches a database back to WAL itself.
fn finish_copy(target: &Connection) -> Result<(), StoreError> {
    target.pragma_update(None, "journal_mode", "DELETE")?;
    verify_integrity(target)
}

fn verify_integrity(connection: &Connection) -> Result<(), StoreError> {
    let verdict: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if verdict == "ok" {
        Ok(())
    } else {
        Err(StoreError::Integrity(verdict))
    }
}
