use std::path::{Path, PathBuf};

use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::Version;

/// Directory layout of one application (README §15.1): packages, data, logs, backups.
#[derive(Debug, Clone)]
pub(crate) struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub(crate) fn new(data_dir: &Path, app_id: Uuid7) -> Self {
        Self {
            root: data_dir.join("apps").join(app_id.to_string()),
        }
    }

    pub(crate) fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    pub(crate) fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }

    fn marker(&self) -> PathBuf {
        self.root.join("last_started_version")
    }

    /// Copies `app.db` aside when the backend is about to start a different version, so a
    /// migration can be undone from a backup (README §17.5).
    pub(crate) fn backup_before_version_change(&self, next: Version) {
        let previous = std::fs::read_to_string(self.marker()).ok();
        let database = self.data().join("app.db");
        let changed = previous
            .as_deref()
            .is_some_and(|v| v.trim() != next.to_string());
        if !changed || !database.exists() {
            return;
        }
        let dir = self.backups();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let name = format!("app-before-{next}-{}.db", Uuid7::generate());
        if let Err(error) = ilmek_store::backup_file(&database, &dir.join(name)) {
            tracing::warn!(error = %error, "pre-update backup failed");
        }
    }

    pub(crate) fn remember_version(&self, version: Version) {
        let _ = std::fs::write(self.marker(), version.to_string());
    }
}
