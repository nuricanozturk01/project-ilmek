//! Name resolution with a bounded offline fallback (PROTOCOL.md §8.3).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use ilmek_core::clock::Clock;
use ilmek_core::session::ClientError;
use ilmek_core::trust::Freshness;
use ilmek_protocol::address::Address;
use ilmek_protocol::wire::{AppRecord, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::connector::Connector;
use crate::error::Error;
use crate::registry_client::RegistryClient;

/// Registry answers are reused for this long before asking again.
const CACHE_TTL_MS: u64 = 60_000;
/// A cached record is never used offline once it is older than this.
const MAX_OFFLINE_AGE_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    record: AppRecord,
    fetched_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The registry answered (or answered within the last minute).
    Registry,
    /// The registry is unreachable; this is a previously verified record. Show it as stale.
    StaleCache,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub record: AppRecord,
    pub source: Source,
}

pub struct Resolver {
    connector: Arc<Connector>,
    clock: Arc<dyn Clock>,
    cache_file: Option<PathBuf>,
    cache: Mutex<HashMap<String, Cached>>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolver").finish_non_exhaustive()
    }
}

impl Resolver {
    /// `cache_file` keeps resolved records across restarts; pass `None` to keep them in memory.
    #[must_use]
    pub fn new(
        connector: Arc<Connector>,
        clock: Arc<dyn Clock>,
        cache_file: Option<PathBuf>,
    ) -> Self {
        let cache = cache_file
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            connector,
            clock,
            cache_file,
            cache: Mutex::new(cache),
        }
    }

    fn key(address: &Address) -> String {
        format!("{}/{}", address.namespace, address.slug)
    }

    pub async fn resolve(&self, address: &Address) -> Result<Resolution, Error> {
        let key = Self::key(address);
        let now = self.clock.now_ms();
        let cached = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
            .cloned();
        if let Some(entry) = cached
            .as_ref()
            .filter(|e| now.saturating_sub(e.fetched_at_ms) < CACHE_TTL_MS)
        {
            return Ok(Resolution {
                record: entry.record.clone(),
                source: Source::Registry,
            });
        }

        match self.ask_registry(address).await {
            Ok(record) => {
                self.remember(&key, &record, now);
                Ok(Resolution {
                    record,
                    source: Source::Registry,
                })
            }
            Err(error) if error.remote_code() == Some(ErrorCode::NotFound) => {
                self.forget(&key);
                Err(error)
            }
            Err(error) if is_unreachable(&error) => {
                let usable = cached.filter(|e| {
                    now.saturating_sub(e.fetched_at_ms) <= MAX_OFFLINE_AGE_MS
                        && self.connector.trust().freshness() == Freshness::Fresh
                });
                usable.map_or(Err(error), |entry| {
                    Ok(Resolution {
                        record: entry.record,
                        source: Source::StaleCache,
                    })
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn ask_registry(&self, address: &Address) -> Result<AppRecord, Error> {
        let registry = RegistryClient::connect(&self.connector).await?;
        let record = registry.resolve(&address.namespace, &address.slug).await;
        registry.close().await;
        record
    }

    fn remember(&self, key: &str, record: &AppRecord, now: u64) {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache.insert(
            key.to_owned(),
            Cached {
                record: record.clone(),
                fetched_at_ms: now,
            },
        );
        self.persist(&cache);
    }

    fn forget(&self, key: &str) {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.remove(key).is_some() {
            self.persist(&cache);
        }
    }

    fn persist(&self, cache: &HashMap<String, Cached>) {
        let Some(path) = &self.cache_file else { return };
        let Ok(bytes) = serde_json::to_vec(cache) else {
            return;
        };
        if let Err(error) = std::fs::write(path, bytes) {
            tracing::warn!(%error, "could not save the name cache");
        }
    }
}

/// `true` for failures that mean "could not ask", as opposed to "was told no".
fn is_unreachable(error: &Error) -> bool {
    matches!(error, Error::Tls(_))
        || matches!(
            error,
            Error::Session(ClientError::Timeout | ClientError::Closed | ClientError::Transport(_))
        )
}
