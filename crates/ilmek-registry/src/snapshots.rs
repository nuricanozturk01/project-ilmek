use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{AppRecord, Cursor};

use crate::error::RegistryError;

const SNAPSHOT_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_SNAPSHOTS: usize = 256;

pub(crate) enum Items {
    Apps {
        query: String,
        limit: u64,
        records: Vec<AppRecord>,
    },
}

struct Snapshot {
    created_ms: u64,
    principal: Uuid7,
    items: std::sync::Arc<Items>,
}

/// Fixed result sets that cursors page through; expire after five minutes.
#[derive(Default)]
pub(crate) struct Snapshots {
    inner: Mutex<HashMap<Uuid7, Snapshot>>,
}

impl Snapshots {
    pub(crate) fn create(&self, principal: Uuid7, now_ms: u64, items: Items) -> Uuid7 {
        let id = Uuid7::generate();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.retain(|_, s| now_ms.saturating_sub(s.created_ms) < SNAPSHOT_TTL_MS);
        let oldest = inner
            .iter()
            .min_by_key(|(_, s)| s.created_ms)
            .map(|(id, _)| *id);
        if let Some(oldest) = oldest.filter(|_| inner.len() >= MAX_SNAPSHOTS) {
            inner.remove(&oldest);
        }
        inner.insert(
            id,
            Snapshot {
                created_ms: now_ms,
                principal,
                items: std::sync::Arc::new(items),
            },
        );
        id
    }

    /// Looks a snapshot up for the cursor's owner; anything else is reported as expired.
    pub(crate) fn get(
        &self,
        id: Uuid7,
        principal: Uuid7,
        now_ms: u64,
    ) -> Result<std::sync::Arc<Items>, RegistryError> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner
            .get(&id)
            .filter(|s| {
                s.principal == principal && now_ms.saturating_sub(s.created_ms) < SNAPSHOT_TTL_MS
            })
            .map(|s| std::sync::Arc::clone(&s.items))
            .ok_or(RegistryError::CursorExpired)
    }
}

pub(crate) fn encode_cursor(snapshot: Uuid7, offset: usize) -> Result<Cursor, RegistryError> {
    format!("{snapshot}_{offset}")
        .parse()
        .map_err(|_| RegistryError::Internal("cursor encoding".into()))
}

pub(crate) fn decode_cursor(cursor: &Cursor) -> Result<(Uuid7, usize), RegistryError> {
    let (id, offset) = cursor
        .as_str()
        .split_once('_')
        .ok_or(RegistryError::CursorExpired)?;
    let id = id.parse().map_err(|_| RegistryError::CursorExpired)?;
    let offset = offset.parse().map_err(|_| RegistryError::CursorExpired)?;
    Ok((id, offset))
}
