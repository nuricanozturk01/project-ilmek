use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use ilmek_runtime::{AppSession, Client};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

/// Where the device identity, package cache and application data live.
///
/// `ILMEK_DATA_DIR` overrides the platform default; tests and multi-profile use rely on it.
pub(crate) fn data_root(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    let root = match std::env::var_os("ILMEK_DATA_DIR") {
        Some(path) => PathBuf::from(path),
        None => app.path().app_data_dir()?,
    };
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

/// Mutable shell state, shared by all commands.
pub(crate) struct Shell {
    pub(crate) root: PathBuf,
    client: Mutex<Option<Arc<Client>>>,
    sessions: Mutex<HashMap<String, Arc<AppSession>>>,
    transfers: Mutex<HashMap<String, CancellationToken>>,
    window_counter: AtomicU64,
}

impl Shell {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            client: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            window_counter: AtomicU64::new(1),
        }
    }

    /// The opened client, loading the stored device identity on first use.
    pub(crate) fn client(&self) -> Option<Arc<Client>> {
        let mut guard = self.client.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() && Client::is_joined(&self.root) {
            *guard = Client::open(&self.root).ok().map(Arc::new);
        }
        guard.clone()
    }

    /// Forgets the opened client so the next call reloads it (after joining a community).
    pub(crate) fn reset_client(&self) {
        *self.client.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    pub(crate) fn next_label(&self) -> String {
        format!(
            "app-{}",
            self.window_counter.fetch_add(1, Ordering::Relaxed)
        )
    }

    pub(crate) fn add_session(&self, label: &str, session: Arc<AppSession>) {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(label.to_owned(), session);
    }

    pub(crate) fn session(&self, label: &str) -> Option<Arc<AppSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(label)
            .cloned()
    }

    pub(crate) fn has_sessions(&self) -> bool {
        !self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }

    pub(crate) fn remove_session(&self, label: &str) -> Option<Arc<AppSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(label)
    }

    pub(crate) fn begin_transfer(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned(), token.clone());
        token
    }

    pub(crate) fn end_transfer(&self, id: &str) {
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
    }

    pub(crate) fn cancel_transfer(&self, id: &str) {
        if let Some(token) = self
            .transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
        {
            token.cancel();
        }
    }
}
