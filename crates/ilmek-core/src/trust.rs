//! Revocation knowledge and its freshness (PROTOCOL.md §3.3, §8.4).

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, PoisonError, RwLock};

use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Cursor, Hash, TrustEntry, TrustKind};
use thiserror::Error;

use crate::clock::Clock;
use crate::peer::PeerCredentials;
use crate::session::{PeerGate, Verdict};
use ilmek_protocol::wire::ErrorCode;

/// A complete snapshot older than this no longer authorizes remote access.
pub const MAX_SNAPSHOT_AGE_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_ENTRIES: usize = 1_000_000;

#[derive(Debug, Error)]
pub enum TrustError {
    #[error("trust page is inconsistent with the snapshot being assembled")]
    InconsistentPage,
    #[error("trust entry has an invalid value")]
    InvalidEntry,
    #[error("snapshot is larger than {MAX_ENTRIES} entries")]
    TooLarge,
    #[error("registry revision went backwards ({received} < {current}); operator review needed")]
    RevisionRegressed { current: u64, received: u64 },
    #[error("fetching a trust page failed: {0}")]
    Fetch(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// No complete snapshot has been installed since start-up.
    Missing,
    Stale,
    Fresh,
}

#[derive(Debug, Clone)]
struct Snapshot {
    revision: u64,
    installed_at_ms: u64,
    certificates: HashSet<String>,
    publisher_keys: HashSet<Hash>,
}

/// One page as returned by `TRUST_PAGE`.
#[derive(Debug, Clone)]
pub struct TrustPageData {
    pub snapshot_id: Uuid7,
    pub revision: u64,
    pub generated_at_ms: u64,
    pub next_cursor: Option<Cursor>,
    pub entries: Vec<TrustEntry>,
}

#[derive(Debug)]
struct Assembly {
    snapshot_id: Uuid7,
    revision: u64,
    certificates: HashSet<String>,
    publisher_keys: HashSet<Hash>,
}

impl Assembly {
    fn start(first: &TrustPageData) -> Self {
        Self {
            snapshot_id: first.snapshot_id,
            revision: first.revision,
            certificates: HashSet::new(),
            publisher_keys: HashSet::new(),
        }
    }

    fn add(&mut self, page: &TrustPageData) -> Result<(), TrustError> {
        if page.snapshot_id != self.snapshot_id || page.revision != self.revision {
            return Err(TrustError::InconsistentPage);
        }
        for entry in &page.entries {
            match entry.kind {
                TrustKind::Certificate => {
                    if !valid_serial(&entry.value) {
                        return Err(TrustError::InvalidEntry);
                    }
                    self.certificates.insert(entry.value.clone());
                }
                TrustKind::PublisherKey => {
                    let key = entry.value.parse().map_err(|_| TrustError::InvalidEntry)?;
                    self.publisher_keys.insert(key);
                }
            }
        }
        if self.certificates.len() + self.publisher_keys.len() > MAX_ENTRIES {
            return Err(TrustError::TooLarge);
        }
        Ok(())
    }
}

fn valid_serial(value: &str) -> bool {
    (2..=40).contains(&value.len())
        && value.len().is_multiple_of(2)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub struct TrustStore {
    clock: Arc<dyn Clock>,
    current: RwLock<Option<Snapshot>>,
}

impl std::fmt::Debug for TrustStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustStore").finish_non_exhaustive()
    }
}

impl TrustStore {
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            current: RwLock::new(None),
        }
    }

    /// Fetches every page, and only then replaces the active snapshot. A failed or partial
    /// refresh changes nothing, including the 24-hour age.
    pub async fn refresh<F, Fut, E>(&self, mut fetch: F) -> Result<(), TrustError>
    where
        F: FnMut(Option<Cursor>) -> Fut,
        Fut: Future<Output = Result<TrustPageData, E>>,
        E: std::fmt::Display,
    {
        let mut cursor = None;
        let mut assembly: Option<Assembly> = None;
        loop {
            let page = fetch(cursor.take())
                .await
                .map_err(|e| TrustError::Fetch(e.to_string()))?;
            let building = assembly.get_or_insert_with(|| Assembly::start(&page));
            building.add(&page)?;
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        let Some(done) = assembly else {
            return Err(TrustError::InconsistentPage);
        };
        self.install(done)
    }

    fn install(&self, assembly: Assembly) -> Result<(), TrustError> {
        let mut current = self.current.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = current.as_ref().filter(|e| assembly.revision < e.revision) {
            return Err(TrustError::RevisionRegressed {
                current: existing.revision,
                received: assembly.revision,
            });
        }
        *current = Some(Snapshot {
            revision: assembly.revision,
            installed_at_ms: self.clock.now_ms(),
            certificates: assembly.certificates,
            publisher_keys: assembly.publisher_keys,
        });
        Ok(())
    }

    #[must_use]
    pub fn freshness(&self) -> Freshness {
        let current = self.current.read().unwrap_or_else(PoisonError::into_inner);
        let Some(snapshot) = current.as_ref() else {
            return Freshness::Missing;
        };
        let now = self.clock.now_ms();
        // A clock that moved backwards makes the age unknowable, so treat it as expired.
        let age = now.checked_sub(snapshot.installed_at_ms);
        match age {
            Some(age) if age <= MAX_SNAPSHOT_AGE_MS => Freshness::Fresh,
            _ => Freshness::Stale,
        }
    }

    #[must_use]
    pub fn certificate_revoked(&self, serial: &str) -> bool {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|snapshot| snapshot.certificates.contains(serial))
    }

    #[must_use]
    pub fn publisher_key_revoked(&self, key_id: &Hash) -> bool {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|snapshot| snapshot.publisher_keys.contains(key_id))
    }

    #[must_use]
    pub fn revision(&self) -> Option<u64> {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|s| s.revision)
    }
}

impl PeerGate for TrustStore {
    fn check(&self, peer: Option<&PeerCredentials>) -> Verdict {
        let Some(peer) = peer else {
            return Verdict::Allow;
        };
        if self.certificate_revoked(&peer.serial) {
            return Verdict::Deny {
                code: ErrorCode::Unauthorized,
                close: true,
            };
        }
        match self.freshness() {
            Freshness::Fresh => Verdict::Allow,
            Freshness::Missing | Freshness::Stale => Verdict::Deny {
                code: ErrorCode::TrustStale,
                close: false,
            },
        }
    }
}
