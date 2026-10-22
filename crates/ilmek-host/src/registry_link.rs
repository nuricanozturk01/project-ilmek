//! Keeps the host registered, its lease fresh and its trust snapshot current.

use std::sync::Arc;
use std::time::Duration;

use ilmek_client::{Connector, RegistryClient};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::Endpoint;
use tokio::time::{interval, sleep};
use tokio_util::sync::CancellationToken;

use crate::store::Store;

const FIRST_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(30);

/// Runs until `shutdown`, reconnecting with exponential backoff after any failure.
///
/// Until the first trust snapshot arrives the connection gate refuses every request, so a host
/// that cannot reach the registry fails closed.
pub(crate) struct Cadence {
    pub(crate) heartbeat: Duration,
    pub(crate) trust: Duration,
}

pub(crate) async fn maintain(
    connector: Arc<Connector>,
    store: Store,
    endpoint: Endpoint,
    cadence: Cadence,
    shutdown: CancellationToken,
) {
    let mut delay = FIRST_RETRY;
    loop {
        match session(&connector, &store, endpoint, &cadence, &shutdown).await {
            Ok(()) => return,
            Err(message) => tracing::warn!(error = %message, "registry link lost; retrying"),
        }
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = sleep(delay) => {}
        }
        delay = (delay * 2).min(MAX_RETRY);
    }
}

async fn session(
    connector: &Connector,
    store: &Store,
    endpoint: Endpoint,
    cadence: &Cadence,
    shutdown: &CancellationToken,
) -> Result<(), String> {
    let registry = RegistryClient::connect(connector)
        .await
        .map_err(|e| e.to_string())?;
    registry
        .refresh_trust(connector.trust())
        .await
        .map_err(|e| e.to_string())?;

    let expected = store.host_revision().await.map_err(|e| e.to_string())?;
    let registered = registry
        .host_register(Uuid7::generate(), endpoint, expected)
        .await
        .map_err(|e| e.to_string())?;
    store
        .set_host_revision(registered.revision)
        .await
        .map_err(|e| e.to_string())?;

    let mut heartbeat = interval(cadence.heartbeat);
    let mut trust = interval(cadence.trust);
    heartbeat.tick().await;
    trust.tick().await;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                registry.close().await;
                return Ok(());
            }
            _ = heartbeat.tick() => {
                registry.heartbeat(registered.revision).await.map_err(|e| e.to_string())?;
            }
            _ = trust.tick() => {
                registry.refresh_trust(connector.trust()).await.map_err(|e| e.to_string())?;
            }
        }
    }
}
