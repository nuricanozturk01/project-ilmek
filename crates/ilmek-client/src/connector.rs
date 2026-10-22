use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ilmek_core::clock::Clock;
use ilmek_core::session::{ClientParams, ClientSession};
use ilmek_core::tls::{ServerTarget, TlsClient};
use ilmek_core::trust::{Freshness, TrustStore};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Mode, ServerRole, Version};

use crate::device::Device;
use crate::error::Error;

const CLIENT_NAME: &str = "ilmek-client";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Opens authenticated connections for one enrolled device.
pub struct Connector {
    device: Arc<Device>,
    tls: TlsClient,
    trust: Arc<TrustStore>,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("peer_id", &self.device.peer_id)
            .finish_non_exhaustive()
    }
}

impl Connector {
    pub fn new(device: Device, clock: Arc<dyn Clock>) -> Result<Self, Error> {
        let tls = TlsClient::new(Some(&device.identity), device.profile.ca_der())?;
        Ok(Self {
            device: Arc::new(device),
            tls,
            trust: Arc::new(TrustStore::new(clock)),
        })
    }

    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    #[must_use]
    pub fn trust(&self) -> &Arc<TrustStore> {
        &self.trust
    }

    /// Connects to the registry. Allowed while trust is stale: this is how trust is refreshed.
    pub async fn connect_registry(&self) -> Result<ClientSession, Error> {
        let profile = &self.device.profile;
        let target = ServerTarget {
            address: SocketAddr::V4(profile.registry_control),
            server_id: profile.registry_id,
        };
        self.open(target, ServerRole::Registry, Mode::Control).await
    }

    /// Connects to an application host; refused unless trust is fresh and the host's
    /// certificate is not revoked.
    pub async fn connect_host(
        &self,
        address: SocketAddr,
        server_id: Uuid7,
        mode: Mode,
    ) -> Result<ClientSession, Error> {
        if self.trust.freshness() != Freshness::Fresh {
            return Err(Error::TrustStale);
        }
        self.open(ServerTarget { address, server_id }, ServerRole::Host, mode)
            .await
    }

    async fn open(
        &self,
        target: ServerTarget,
        role: ServerRole,
        mode: Mode,
    ) -> Result<ClientSession, Error> {
        let connection = self.tls.connect(target).await?;
        if role == ServerRole::Host && self.trust.certificate_revoked(&connection.server.serial) {
            return Err(Error::PeerRevoked);
        }
        let params = ClientParams {
            mode,
            community_id: self.device.profile.community_id,
            expected_server_id: target.server_id,
            expected_role: role,
            client_name: CLIENT_NAME.to_owned(),
            client_version: Version::new(1, 0, 0),
            request_timeout: REQUEST_TIMEOUT,
        };
        Ok(ClientSession::open(connection.stream, params).await?)
    }
}
