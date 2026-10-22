#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ilmek_core::clock::Clock;
use ilmek_core::peer::PeerCredentials;
use ilmek_core::session::{PeerGate, Verdict};
use ilmek_core::trust::{Freshness, MAX_SNAPSHOT_AGE_MS, TrustError, TrustPageData, TrustStore};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Cursor, ErrorCode, TrustEntry, TrustKind};

#[derive(Debug)]
struct ManualClock(AtomicU64);

impl ManualClock {
    fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::Relaxed);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

fn store() -> (Arc<ManualClock>, TrustStore) {
    let clock = Arc::new(ManualClock(AtomicU64::new(1_000_000)));
    (Arc::clone(&clock), TrustStore::new(clock))
}

fn peer(serial: &str) -> PeerCredentials {
    PeerCredentials {
        id: Uuid7::generate(),
        serial: serial.into(),
    }
}

fn cert(serial: &str) -> TrustEntry {
    TrustEntry {
        kind: TrustKind::Certificate,
        value: serial.into(),
        revoked_at_ms: 1,
    }
}

fn page(id: Uuid7, revision: u64, entries: Vec<TrustEntry>, next: Option<&str>) -> TrustPageData {
    TrustPageData {
        snapshot_id: id,
        revision,
        generated_at_ms: 0,
        next_cursor: next.map(|c| c.parse::<Cursor>().unwrap()),
        entries,
    }
}

async fn install(store: &TrustStore, revision: u64, entries: Vec<TrustEntry>) {
    let id = Uuid7::generate();
    store
        .refresh(|_| {
            let data = page(id, revision, entries.clone(), None);
            async move { Ok::<_, String>(data) }
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn nothing_is_authorized_before_the_first_complete_snapshot() {
    let (_, store) = store();
    assert_eq!(store.freshness(), Freshness::Missing);
    assert_eq!(
        store.check(Some(&peer("0a"))),
        Verdict::Deny {
            code: ErrorCode::TrustStale,
            close: false
        }
    );
    install(&store, 1, vec![]).await;
    assert_eq!(store.check(Some(&peer("0a"))), Verdict::Allow);
}

#[tokio::test]
async fn enrollment_connections_have_no_peer_and_are_not_gated() {
    let (_, store) = store();
    assert_eq!(store.check(None), Verdict::Allow);
}

#[tokio::test]
async fn a_snapshot_takes_effect_only_after_its_last_page() {
    let (_, store) = store();
    let id = Uuid7::generate();
    let store_ref = &store;
    let mut calls = 0;
    store
        .refresh(|cursor| {
            calls += 1;
            let seen_before_last_page = store_ref.certificate_revoked("aa");
            let data = if cursor.is_none() {
                page(id, 1, vec![cert("aa")], Some("p2"))
            } else {
                page(id, 1, vec![cert("bb")], None)
            };
            async move {
                assert!(!seen_before_last_page, "partial snapshot became visible");
                Ok::<_, String>(data)
            }
        })
        .await
        .unwrap();
    assert_eq!(calls, 2);
    assert!(store.certificate_revoked("aa") && store.certificate_revoked("bb"));
}

#[tokio::test]
async fn a_failed_refresh_changes_nothing_and_does_not_reset_the_age() {
    let (clock, store) = store();
    install(&store, 1, vec![cert("aa")]).await;
    clock.set(1_000_000 + MAX_SNAPSHOT_AGE_MS - 1);
    assert_eq!(store.freshness(), Freshness::Fresh);

    let id = Uuid7::generate();
    let mut first = true;
    let error = store
        .refresh(|_| {
            let outcome = if std::mem::take(&mut first) {
                Ok(page(id, 2, vec![cert("cc")], Some("p2")))
            } else {
                Err("link down".to_owned())
            };
            async move { outcome }
        })
        .await
        .unwrap_err();
    assert!(matches!(error, TrustError::Fetch(_)));
    assert!(!store.certificate_revoked("cc"));
    assert_eq!(store.revision(), Some(1));

    clock.set(1_000_000 + MAX_SNAPSHOT_AGE_MS + 1);
    assert_eq!(store.freshness(), Freshness::Stale);
    assert_eq!(
        store.check(Some(&peer("0a"))),
        Verdict::Deny {
            code: ErrorCode::TrustStale,
            close: false
        }
    );
}

#[tokio::test]
async fn a_clock_that_moves_backwards_makes_the_snapshot_stale() {
    let (clock, store) = store();
    install(&store, 1, vec![]).await;
    clock.set(500_000);
    assert_eq!(store.freshness(), Freshness::Stale);
}

#[tokio::test]
async fn revoked_certificates_are_denied_and_the_session_closed() {
    let (_, store) = store();
    install(&store, 1, vec![cert("deadbeef")]).await;
    assert_eq!(
        store.check(Some(&peer("deadbeef"))),
        Verdict::Deny {
            code: ErrorCode::Unauthorized,
            close: true
        }
    );
    assert_eq!(store.check(Some(&peer("0102"))), Verdict::Allow);
}

#[tokio::test]
async fn revision_regression_and_inconsistent_pages_are_rejected() {
    let (_, store) = store();
    install(&store, 5, vec![]).await;

    let error = store
        .refresh(|_| {
            let data = page(Uuid7::generate(), 4, vec![], None);
            async move { Ok::<_, String>(data) }
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        TrustError::RevisionRegressed {
            current: 5,
            received: 4
        }
    ));

    let mut pages = vec![
        page(Uuid7::generate(), 6, vec![], Some("p2")),
        page(Uuid7::generate(), 6, vec![], None),
    ]
    .into_iter();
    let error = store
        .refresh(|_| {
            let data = pages.next().unwrap();
            async move { Ok::<_, String>(data) }
        })
        .await
        .unwrap_err();
    assert!(matches!(error, TrustError::InconsistentPage));
}

#[tokio::test]
async fn malformed_entries_are_rejected() {
    let (_, store) = store();
    for bad in ["A1", "abc", "zz", "", &"a".repeat(42)] {
        let error = store
            .refresh(|_| {
                let data = page(Uuid7::generate(), 1, vec![cert(bad)], None);
                async move { Ok::<_, String>(data) }
            })
            .await
            .unwrap_err();
        assert!(matches!(error, TrustError::InvalidEntry), "{bad:?}");
    }
}
