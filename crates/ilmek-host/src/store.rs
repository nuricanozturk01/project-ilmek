//! Typed access to the host database.

use std::path::PathBuf;

use ilmek_identity::PublisherKey;
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Hash, Name, Version};
use ilmek_store::rusqlite::{Connection, OptionalExtension, Row, params};
use ilmek_store::{Database, Unsigned};

use crate::error::HostError;
use crate::schema::MIGRATIONS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desired {
    Running,
    Stopped,
}

impl Desired {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Installed,
    Starting,
    Running,
    Stopped,
    Failed,
    Updating,
}

impl AppState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Updating => "updating",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [
            Self::Installed,
            Self::Starting,
            Self::Running,
            Self::Stopped,
            Self::Failed,
            Self::Updating,
        ]
        .into_iter()
        .find(|state| state.as_str() == text)
    }
}

#[derive(Debug, Clone)]
pub struct AppRow {
    pub app_id: Uuid7,
    pub namespace: Name,
    pub slug: Name,
    pub publisher_id: Uuid7,
    pub name: String,
    pub backend_required: bool,
    pub active_version: Version,
    pub desired: Desired,
    pub restart_generation: u64,
}

#[derive(Debug, Clone)]
pub struct VersionRow {
    pub version: Version,
    pub manifest_hash: Hash,
    pub directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BackendRow {
    pub executable: PathBuf,
    pub args: Vec<String>,
    /// Client package versions the backend still serves; the active version is always accepted.
    pub accepted_versions: Vec<Version>,
}

#[derive(Debug, Clone)]
pub struct StatusRow {
    pub state: AppState,
    pub pid: Option<u32>,
    pub restarts: u64,
    pub last_error: Option<String>,
    pub updated_at_ms: u64,
}

type StatusColumns = (String, Option<u32>, Unsigned, Option<String>, Unsigned);

#[derive(Debug, Clone)]
pub struct Store {
    db: Database,
}

fn corrupt<T, E>(result: Result<T, E>, what: &'static str) -> Result<T, HostError> {
    result.map_err(|_| HostError::Corrupt(what))
}

const APP_COLUMNS: &str = "app_id, namespace, slug, publisher_id, name, backend_required, \
                           active_version, desired, restart_generation";

fn app_from_row(row: &Row<'_>) -> ilmek_store::rusqlite::Result<Result<AppRow, HostError>> {
    let raw: [String; 4] = [row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?];
    let (name, backend_required): (String, i64) = (row.get(4)?, row.get(5)?);
    let (active, desired): (String, String) = (row.get(6)?, row.get(7)?);
    let generation: Unsigned = row.get(8)?;
    let build = || -> Result<AppRow, HostError> {
        Ok(AppRow {
            app_id: corrupt(raw[0].parse(), "app id")?,
            namespace: corrupt(raw[1].parse(), "namespace")?,
            slug: corrupt(raw[2].parse(), "slug")?,
            publisher_id: corrupt(raw[3].parse(), "publisher id")?,
            name,
            backend_required: backend_required != 0,
            active_version: corrupt(active.parse(), "version")?,
            desired: if desired == "running" {
                Desired::Running
            } else {
                Desired::Stopped
            },
            restart_generation: generation.0,
        })
    };
    Ok(build())
}

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self, HostError> {
        Ok(Self {
            db: Database::open(path, MIGRATIONS)?,
        })
    }

    pub async fn add_trusted_publisher(
        &self,
        key: &PublisherKey,
        now_ms: u64,
    ) -> Result<String, HostError> {
        let (key_id, encoded) = (key.key_id(), key.to_base64url());
        let stored_id = key_id.clone();
        self.db
            .call(move |c| {
                c.execute(
                    "INSERT OR IGNORE INTO trusted_publishers (key_id, public_key, added_at_ms)
                     VALUES (?1, ?2, ?3)",
                    params![stored_id, encoded, Unsigned(now_ms)],
                )?;
                Ok::<_, HostError>(())
            })
            .await?;
        Ok(key_id)
    }

    pub async fn trusted_publisher(&self, key_id: Hash) -> Result<Option<PublisherKey>, HostError> {
        let found: Option<String> = self
            .db
            .call(move |c| {
                Ok::<_, HostError>(
                    c.query_row(
                        "SELECT public_key FROM trusted_publishers WHERE key_id = ?1",
                        params![key_id.to_string()],
                        |row| row.get(0),
                    )
                    .optional()?,
                )
            })
            .await?;
        found
            .map(|text| corrupt(PublisherKey::from_base64url(&text), "trusted key"))
            .transpose()
    }

    pub async fn app(&self, app_id: Uuid7) -> Result<Option<AppRow>, HostError> {
        self.db
            .call(move |c| {
                let sql = format!("SELECT {APP_COLUMNS} FROM apps WHERE app_id = ?1");
                c.query_row(&sql, params![app_id.to_string()], app_from_row)
                    .optional()?
                    .transpose()
            })
            .await
    }

    pub async fn apps(&self) -> Result<Vec<AppRow>, HostError> {
        self.db
            .call(|c| {
                let sql = format!("SELECT {APP_COLUMNS} FROM apps ORDER BY namespace, slug");
                let mut statement = c.prepare(&sql)?;
                let rows = statement.query_map([], app_from_row)?;
                rows.map(|row| row?).collect()
            })
            .await
    }

    pub async fn version(
        &self,
        app_id: Uuid7,
        version: Version,
    ) -> Result<Option<VersionRow>, HostError> {
        self.db.call(move |c| version_row(c, app_id, version)).await
    }

    pub async fn versions(&self, app_id: Uuid7) -> Result<Vec<VersionRow>, HostError> {
        self.db
            .call(move |c| {
                let mut statement = c.prepare(
                    "SELECT version, manifest_hash, directory FROM versions WHERE app_id = ?1",
                )?;
                let rows = statement.query_map(params![app_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                let mut versions = Vec::new();
                for row in rows {
                    let (version, hash, directory) = row?;
                    versions.push(VersionRow {
                        version: corrupt(version.parse(), "version")?,
                        manifest_hash: corrupt(hash.parse(), "manifest hash")?,
                        directory: PathBuf::from(directory),
                    });
                }
                versions.sort_by_key(|v| v.version);
                Ok(versions)
            })
            .await
    }

    /// Records a new application together with its first version.
    pub async fn insert_app(
        &self,
        app: AppRow,
        version: VersionRow,
        now_ms: u64,
    ) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                let tx = c.transaction()?;
                let taken: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM apps WHERE app_id = ?1 OR (namespace = ?2 AND slug = ?3))",
                    params![app.app_id.to_string(), app.namespace.as_str(), app.slug.as_str()],
                    |row| row.get(0),
                )?;
                if taken {
                    return Err(HostError::Conflict("application id or address already installed"));
                }
                tx.execute(
                    "INSERT INTO apps (app_id, namespace, slug, publisher_id, name, backend_required,
                         active_version, desired, restart_generation, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'stopped', 0, ?8)",
                    params![
                        app.app_id.to_string(),
                        app.namespace.as_str(),
                        app.slug.as_str(),
                        app.publisher_id.to_string(),
                        app.name,
                        i64::from(app.backend_required),
                        app.active_version.to_string(),
                        Unsigned(now_ms)
                    ],
                )?;
                insert_version(&tx, app.app_id, &version, now_ms)?;
                tx.execute(
                    "INSERT INTO status (app_id, state, restarts, updated_at_ms)
                     VALUES (?1, 'installed', 0, ?2)",
                    params![app.app_id.to_string(), Unsigned(now_ms)],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
    }

    /// Adds a verified version and makes it active in a single transaction.
    pub async fn activate_new_version(
        &self,
        app_id: Uuid7,
        version: VersionRow,
        name: String,
        now_ms: u64,
    ) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                let tx = c.transaction()?;
                insert_version(&tx, app_id, &version, now_ms)?;
                tx.execute(
                    "UPDATE apps SET active_version = ?2, name = ?3 WHERE app_id = ?1",
                    params![app_id.to_string(), version.version.to_string(), name],
                )?;
                tx.commit()?;
                Ok::<_, HostError>(())
            })
            .await
    }

    pub async fn set_active_version(
        &self,
        app_id: Uuid7,
        version: Version,
    ) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                let known = version_row(c, app_id, version)?.is_some();
                if !known {
                    return Err(HostError::UnknownVersion);
                }
                c.execute(
                    "UPDATE apps SET active_version = ?2 WHERE app_id = ?1",
                    params![app_id.to_string(), version.to_string()],
                )?;
                Ok(())
            })
            .await
    }

    pub async fn set_desired(
        &self,
        app_id: Uuid7,
        desired: Desired,
        restart: bool,
    ) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                let updated = c.execute(
                    "UPDATE apps SET desired = ?2,
                         restart_generation = restart_generation + ?3 WHERE app_id = ?1",
                    params![app_id.to_string(), desired.as_str(), i64::from(restart)],
                )?;
                if updated == 0 {
                    return Err(HostError::UnknownApp);
                }
                Ok(())
            })
            .await
    }

    pub async fn set_backend(&self, app_id: Uuid7, backend: BackendRow) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                if c.query_row(
                    "SELECT 1 FROM apps WHERE app_id = ?1",
                    params![app_id.to_string()],
                    |_| Ok(()),
                )
                .optional()?
                .is_none()
                {
                    return Err(HostError::UnknownApp);
                }
                let args = corrupt(serde_json::to_string(&backend.args), "args")?;
                let accepted = corrupt(
                    serde_json::to_string(
                        &backend
                            .accepted_versions
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>(),
                    ),
                    "accepted versions",
                )?;
                c.execute(
                    "INSERT INTO backends (app_id, executable, args, accepted_versions)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(app_id) DO UPDATE SET executable = excluded.executable,
                         args = excluded.args, accepted_versions = excluded.accepted_versions",
                    params![
                        app_id.to_string(),
                        backend.executable.display().to_string(),
                        args,
                        accepted
                    ],
                )?;
                Ok(())
            })
            .await
    }

    pub async fn backend(&self, app_id: Uuid7) -> Result<Option<BackendRow>, HostError> {
        self.db
            .call(move |c| {
                let row: Option<(String, String, String)> = c
                    .query_row(
                        "SELECT executable, args, accepted_versions FROM backends WHERE app_id = ?1",
                        params![app_id.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                row.map(|(executable, args, accepted)| {
                    let accepted: Vec<String> = corrupt(serde_json::from_str(&accepted), "accepted versions")?;
                    Ok(BackendRow {
                        executable: PathBuf::from(executable),
                        args: corrupt(serde_json::from_str(&args), "backend args")?,
                        accepted_versions: accepted
                            .iter()
                            .map(|v| corrupt(v.parse(), "accepted version"))
                            .collect::<Result<_, _>>()?,
                    })
                })
                .transpose()
            })
            .await
    }

    pub async fn write_status(&self, app_id: Uuid7, status: StatusRow) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                c.execute(
                    "INSERT INTO status (app_id, state, pid, restarts, last_error, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(app_id) DO UPDATE SET state = excluded.state, pid = excluded.pid,
                         restarts = excluded.restarts, last_error = excluded.last_error,
                         updated_at_ms = excluded.updated_at_ms",
                    params![
                        app_id.to_string(),
                        status.state.as_str(),
                        status.pid,
                        Unsigned(status.restarts),
                        status.last_error,
                        Unsigned(status.updated_at_ms)
                    ],
                )?;
                Ok::<_, HostError>(())
            })
            .await
    }

    pub async fn status(&self, app_id: Uuid7) -> Result<Option<StatusRow>, HostError> {
        self.db
            .call(move |c| {
                let row: Option<StatusColumns> = c
                    .query_row(
                        "SELECT state, pid, restarts, last_error, updated_at_ms FROM status WHERE app_id = ?1",
                        params![app_id.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )
                    .optional()?;
                row.map(|(state, pid, restarts, last_error, updated)| {
                    Ok(StatusRow {
                        state: AppState::parse(&state).ok_or(HostError::Corrupt("state"))?,
                        pid,
                        restarts: restarts.0,
                        last_error,
                        updated_at_ms: updated.0,
                    })
                })
                .transpose()
            })
            .await
    }

    /// Forgets an application. Its data directory is left for the operator to purge.
    pub async fn remove_app(&self, app_id: Uuid7) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                let tx = c.transaction()?;
                let id = app_id.to_string();
                for table in ["status", "backends", "versions"] {
                    tx.execute(
                        &format!("DELETE FROM {table} WHERE app_id = ?1"),
                        params![id],
                    )?;
                }
                if tx.execute("DELETE FROM apps WHERE app_id = ?1", params![id])? == 0 {
                    return Err(HostError::UnknownApp);
                }
                tx.commit()?;
                Ok(())
            })
            .await
    }

    pub async fn host_revision(&self) -> Result<u64, HostError> {
        self.db
            .call(|c| {
                let revision: Unsigned =
                    c.query_row("SELECT host_revision FROM registry_state", [], |row| {
                        row.get(0)
                    })?;
                Ok::<_, HostError>(revision.0)
            })
            .await
    }

    pub async fn set_host_revision(&self, revision: u64) -> Result<(), HostError> {
        self.db
            .call(move |c| {
                c.execute(
                    "UPDATE registry_state SET host_revision = ?1",
                    params![Unsigned(revision)],
                )?;
                Ok::<_, HostError>(())
            })
            .await
    }

    #[must_use]
    pub fn database(&self) -> &Database {
        &self.db
    }
}

fn version_row(
    connection: &Connection,
    app_id: Uuid7,
    version: Version,
) -> Result<Option<VersionRow>, HostError> {
    let row: Option<(String, String)> = connection
        .query_row(
            "SELECT manifest_hash, directory FROM versions WHERE app_id = ?1 AND version = ?2",
            params![app_id.to_string(), version.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(hash, directory)| {
        Ok(VersionRow {
            version,
            manifest_hash: corrupt(hash.parse(), "manifest hash")?,
            directory: PathBuf::from(directory),
        })
    })
    .transpose()
}

fn insert_version(
    transaction: &ilmek_store::rusqlite::Transaction<'_>,
    app_id: Uuid7,
    version: &VersionRow,
    now_ms: u64,
) -> Result<(), HostError> {
    transaction.execute(
        "INSERT INTO versions (app_id, version, manifest_hash, directory, installed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            app_id.to_string(),
            version.version.to_string(),
            version.manifest_hash.to_string(),
            version.directory.display().to_string(),
            Unsigned(now_ms)
        ],
    )?;
    Ok(())
}
