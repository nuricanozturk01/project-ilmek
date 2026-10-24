use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

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
    /// When each live application tab was last the visible one.
    last_shown: Mutex<HashMap<String, Instant>>,
    /// The visible application tab, if the current tab is one.
    shown: Mutex<Option<String>>,
    /// `ilmek://` addresses handed to the program (links, command line) that the UI has not
    /// picked up yet.
    pending: Mutex<Vec<String>>,
}

impl Shell {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            client: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            window_counter: AtomicU64::new(1),
            last_shown: Mutex::new(HashMap::new()),
            shown: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
        }
    }

    /// Records which application tab is on screen (`None`: a shell page is).
    pub(crate) fn set_shown(&self, label: Option<&str>) {
        *self.shown.lock().unwrap_or_else(PoisonError::into_inner) = label.map(str::to_owned);
        if let Some(label) = label {
            self.last_shown
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(label.to_owned(), Instant::now());
        }
    }

    pub(crate) fn shown(&self) -> Option<String> {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn forget_tab(&self, label: &str) {
        self.last_shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(label);
        let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
        if shown.as_deref() == Some(label) {
            *shown = None;
        }
    }

    /// Live tabs that are not on screen, least recently shown first, with how long ago.
    pub(crate) fn background_tabs(&self) -> Vec<(String, std::time::Duration)> {
        let shown = self.shown();
        let seen = self
            .last_shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut tabs: Vec<(String, std::time::Duration)> = self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .filter(|label| shown.as_deref() != Some(label.as_str()))
            .map(|label| {
                let idle = seen
                    .get(label)
                    .map_or(std::time::Duration::MAX, Instant::elapsed);
                (label.clone(), idle)
            })
            .collect();
        tabs.sort_by_key(|(_, idle)| std::cmp::Reverse(*idle));
        tabs
    }

    pub(crate) fn live_tabs(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub(crate) fn queue_address(&self, address: String) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(address);
    }

    pub(crate) fn take_pending(&self) -> Vec<String> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(PoisonError::into_inner))
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

    /// The session whose private host name is `host` (how the content scheme tells tabs apart).
    pub(crate) fn session_by_host(&self, host: &str) -> Option<Arc<AppSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .find(|session| session.host_name() == host)
            .cloned()
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
