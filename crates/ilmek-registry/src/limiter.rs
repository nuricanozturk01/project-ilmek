use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};

use crate::error::RegistryError;

const WINDOW_MS: u64 = 60_000;
const MAX_FAILURES: u32 = 5;
const MAX_TRACKED_SOURCES: usize = 4096;

/// Per-source limit on failed enrollment attempts (PROTOCOL.md §8.1).
#[derive(Debug, Default)]
pub(crate) struct FailureLimiter {
    sources: Mutex<HashMap<IpAddr, (u64, u32)>>,
}

impl FailureLimiter {
    pub(crate) fn check(&self, source: Option<IpAddr>, now_ms: u64) -> Result<(), RegistryError> {
        let Some(source) = source else { return Ok(()) };
        let sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        match sources.get(&source) {
            Some(&(started, failures))
                if now_ms.saturating_sub(started) < WINDOW_MS && failures >= MAX_FAILURES =>
            {
                Err(RegistryError::RateLimited {
                    retry_after_ms: WINDOW_MS - now_ms.saturating_sub(started),
                })
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn record_failure(&self, source: Option<IpAddr>, now_ms: u64) {
        let Some(source) = source else { return };
        let mut sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        if sources.len() >= MAX_TRACKED_SOURCES {
            sources.retain(|_, (started, _)| now_ms.saturating_sub(*started) < WINDOW_MS);
        }
        let entry = sources.entry(source).or_insert((now_ms, 0));
        if now_ms.saturating_sub(entry.0) >= WINDOW_MS {
            *entry = (now_ms, 0);
        }
        entry.1 += 1;
    }
}
