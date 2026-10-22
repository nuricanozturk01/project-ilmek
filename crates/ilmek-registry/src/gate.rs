use std::collections::HashSet;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use ilmek_core::peer::PeerCredentials;
use ilmek_core::session::{PeerGate, Verdict};
use ilmek_protocol::wire::ErrorCode;
use ilmek_store::StoreError;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use crate::error::RegistryError;
use crate::registry::Registry;

/// Refuses revoked certificates. The registry owns the revocation table, so it never goes
/// stale the way a client's trust snapshot can.
#[derive(Debug, Default)]
pub struct RegistryGate {
    revoked: RwLock<HashSet<String>>,
}

impl RegistryGate {
    fn replace(&self, serials: HashSet<String>) {
        *self.revoked.write().unwrap_or_else(PoisonError::into_inner) = serials;
    }
}

impl PeerGate for RegistryGate {
    fn check(&self, peer: Option<&PeerCredentials>) -> Verdict {
        let revoked = peer.is_some_and(|peer| {
            self.revoked
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(&peer.serial)
        });
        if revoked {
            Verdict::Deny {
                code: ErrorCode::Unauthorized,
                close: true,
            }
        } else {
            Verdict::Allow
        }
    }
}

fn load_revoked(registry: &Registry) -> Result<HashSet<String>, RegistryError> {
    registry.db.call_blocking(|connection| {
        let mut statement =
            connection.prepare("SELECT value FROM revocations WHERE kind = 'certificate'")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(|e| RegistryError::from(StoreError::from(e)))
    })
}

impl Registry {
    /// Builds the gate and keeps it current until `shutdown`.
    pub fn start_gate(
        self: &Arc<Self>,
        shutdown: CancellationToken,
    ) -> Result<Arc<RegistryGate>, RegistryError> {
        let gate = Arc::new(RegistryGate::default());
        gate.replace(load_revoked(self)?);

        let registry = Arc::clone(self);
        let watched = Arc::clone(&gate);
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(
                registry.settings.revocation_refresh_ms,
            ));
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => return,
                    _ = tick.tick() => {}
                }
                let registry = Arc::clone(&registry);
                if let Ok(Ok(serials)) =
                    tokio::task::spawn_blocking(move || load_revoked(&registry)).await
                {
                    watched.replace(serials);
                }
            }
        });
        Ok(gate)
    }
}
