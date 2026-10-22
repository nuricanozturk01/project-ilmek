use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use ilmek_protocol::Uuid7;
use tokio_util::sync::CancellationToken;

use crate::link::BackendLink;

/// A live event subscription owned by one network session.
#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) session_id: Uuid7,
    pub(crate) link: Arc<BackendLink>,
    pub(crate) stop: CancellationToken,
}

#[derive(Default)]
pub(crate) struct Subscriptions {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    by_id: HashMap<Uuid7, Entry>,
    per_session: HashMap<Uuid7, usize>,
}

impl Subscriptions {
    /// Registers a subscription unless the session is at its limit.
    pub(crate) fn add(&self, id: Uuid7, entry: Entry, limit: u64) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let count = inner
            .per_session
            .get(&entry.session_id)
            .copied()
            .unwrap_or(0);
        if u64::try_from(count).unwrap_or(u64::MAX) >= limit {
            return false;
        }
        *inner.per_session.entry(entry.session_id).or_insert(0) += 1;
        inner.by_id.insert(id, entry);
        true
    }

    pub(crate) fn count(&self, session_id: Uuid7) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .per_session
            .get(&session_id)
            .copied()
            .unwrap_or(0)
    }

    /// The subscription, only if `session_id` owns it; foreign ids look like unknown ones.
    pub(crate) fn owned(&self, id: Uuid7, session_id: Uuid7) -> Option<Entry> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner
            .by_id
            .get(&id)
            .filter(|entry| entry.session_id == session_id)
            .cloned()
    }

    pub(crate) fn remove(&self, id: Uuid7) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = inner.by_id.remove(&id) {
            if let Some(count) = inner.per_session.get_mut(&entry.session_id) {
                *count -= 1;
                if *count == 0 {
                    inner.per_session.remove(&entry.session_id);
                }
            }
        }
    }
}
