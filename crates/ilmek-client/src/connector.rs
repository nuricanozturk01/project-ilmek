use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ilmek_core::net;
use ilmek_core::session::{ClientParams, ClientSession};
use ilmek_protocol::wire::{Mode, ServerRole, Version};

use crate::device::Device;
use crate::error::Error;

const CLIENT_NAME: &str = "ilmek-client";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Opens connections for one device.
pub struct Connector {
    device: Arc<Device>,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("peer_id", &self.device.peer_id)
            .finish_non_exhaustive()
    }
}

impl Connector {
    #[must_use]
    pub fn new(device: Device) -> Self {
        Self {
            device: Arc::new(device),
        }
    }

    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Connects to the registry named in the community profile.
    pub async fn connect_registry(&self) -> Result<ClientSession, Error> {
        let address = SocketAddr::V4(self.device.profile.registry);
        self.open(address, ServerRole::Registry, Mode::Control)
            .await
    }

    /// The address to dial for a host that registered `endpoint`. A host on the registry's own
    /// machine registers a loopback address; other devices reach it on the registry's address.
    #[must_use]
    pub fn host_address(&self, endpoint: &ilmek_protocol::wire::Endpoint) -> SocketAddr {
        let registry = *self.device.profile.registry.ip();
        let ip = if endpoint.ip.is_loopback() && !registry.is_loopback() {
            registry
        } else {
            endpoint.ip
        };
        SocketAddr::from((ip, endpoint.port))
    }

    /// Connects to an application host.
    pub async fn connect_host(
        &self,
        address: SocketAddr,
        mode: Mode,
    ) -> Result<ClientSession, Error> {
        self.open(address, ServerRole::Host, mode).await
    }

    async fn open(
        &self,
        address: SocketAddr,
        role: ServerRole,
        mode: Mode,
    ) -> Result<ClientSession, Error> {
        let stream = net::connect(address).await?;
        let params = ClientParams {
            mode,
            community_id: self.device.profile.community_id,
            client_id: self.device.peer_id,
            expected_role: role,
            client_name: CLIENT_NAME.to_owned(),
            client_version: Version::new(1, 0, 0),
            request_timeout: REQUEST_TIMEOUT,
        };
        Ok(ClientSession::open(stream, params).await?)
    }
}

#[cfg(test)]
mod tests {
    use ilmek_core::community::CommunityProfile;
    use ilmek_protocol::Uuid7;
    use ilmek_protocol::wire::Endpoint;

    use super::*;

    fn connector(registry: [u8; 4]) -> Connector {
        let profile = CommunityProfile::new(
            Uuid7::generate(),
            Uuid7::generate(),
            std::net::SocketAddrV4::new(registry.into(), 47401),
        );
        Connector::new(Device {
            profile,
            peer_id: Uuid7::generate(),
        })
    }

    fn endpoint(ip: [u8; 4]) -> Endpoint {
        Endpoint {
            ip: ip.into(),
            port: 47402,
        }
    }

    #[test]
    fn a_host_on_the_registry_machine_is_reached_on_the_registry_address() {
        let lan = connector([192, 168, 1, 10]);
        assert_eq!(
            lan.host_address(&endpoint([127, 0, 0, 1])).to_string(),
            "192.168.1.10:47402"
        );
    }

    #[test]
    fn other_addresses_are_used_as_registered() {
        let lan = connector([192, 168, 1, 10]);
        assert_eq!(
            lan.host_address(&endpoint([192, 168, 1, 20])).to_string(),
            "192.168.1.20:47402"
        );
        let local = connector([127, 0, 0, 1]);
        assert_eq!(
            local.host_address(&endpoint([127, 0, 0, 1])).to_string(),
            "127.0.0.1:47402"
        );
    }
}
