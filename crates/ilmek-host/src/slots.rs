use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use ilmek_protocol::Uuid7;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::link::BackendLink;

/// Bounds concurrent and queued work per application (REQUIREMENTS §15.2).
#[derive(Debug)]
pub struct CallGate {
    permits: Arc<Semaphore>,
    waiting: AtomicUsize,
    queue_depth: usize,
}

impl CallGate {
    fn new(max_inflight: usize, queue_depth: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_inflight)),
            waiting: AtomicUsize::new(0),
            queue_depth,
        }
    }

    /// Waits for a work slot, or returns `None` when the waiting queue is already full.
    pub async fn enter(&self) -> Option<OwnedSemaphorePermit> {
        if let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() {
            return Some(permit);
        }
        if self.waiting.fetch_add(1, Ordering::AcqRel) >= self.queue_depth {
            self.waiting.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        let permit = Arc::clone(&self.permits).acquire_owned().await.ok();
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        permit
    }
}

/// Everything the network service needs to reach one application's backend.
#[derive(Debug)]
pub struct AppSlot {
    link: RwLock<Option<Arc<BackendLink>>>,
    pub gate: CallGate,
}

impl AppSlot {
    fn new(max_inflight: usize, queue_depth: usize) -> Self {
        Self {
            link: RwLock::new(None),
            gate: CallGate::new(max_inflight, queue_depth),
        }
    }

    #[must_use]
    pub fn link(&self) -> Option<Arc<BackendLink>> {
        self.link
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_link(&self, link: Option<Arc<BackendLink>>) {
        *self.link.write().unwrap_or_else(PoisonError::into_inner) = link;
    }
}

#[derive(Debug)]
pub(crate) struct Slots {
    inner: RwLock<HashMap<Uuid7, Arc<AppSlot>>>,
    max_inflight: usize,
    queue_depth: usize,
}

impl Slots {
    #[must_use]
    pub(crate) fn new(max_inflight: usize, queue_depth: usize) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            max_inflight,
            queue_depth,
        }
    }

    pub(crate) fn ensure(&self, app_id: Uuid7) -> Arc<AppSlot> {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(
            inner
                .entry(app_id)
                .or_insert_with(|| Arc::new(AppSlot::new(self.max_inflight, self.queue_depth))),
        )
    }

    #[must_use]
    pub(crate) fn get(&self, app_id: Uuid7) -> Option<Arc<AppSlot>> {
        self.inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&app_id)
            .cloned()
    }

    pub(crate) fn remove(&self, app_id: Uuid7) {
        self.inner
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&app_id);
    }
}
