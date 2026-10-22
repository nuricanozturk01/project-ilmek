#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::net::{Ipv4Addr, SocketAddr};

use ilmek_core::tls::{ServerTarget, TlsServer};
use ilmek_identity::{CertificateAuthority, DeviceKey, Identity, LeafSpec};
use ilmek_protocol::Uuid7;
use time::{Duration, OffsetDateTime};
use tokio::net::TcpListener;

pub(crate) const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;

pub(crate) struct Community {
    pub(crate) ca: CertificateAuthority,
}

pub(crate) struct Peer {
    pub(crate) id: Uuid7,
    pub(crate) identity: Identity,
}

impl Community {
    pub(crate) fn new() -> Self {
        let ca = CertificateAuthority::generate(
            Uuid7::generate(),
            OffsetDateTime::now_utc() - Duration::minutes(1),
            Duration::days(365),
        )
        .unwrap();
        Self { ca }
    }

    pub(crate) fn ca_der(&self) -> &[u8] {
        self.ca.certificate_der()
    }

    pub(crate) fn peer(&self, server_ip: Option<Ipv4Addr>) -> Peer {
        self.peer_valid_for(
            server_ip,
            OffsetDateTime::now_utc() - Duration::minutes(1),
            Duration::days(30),
        )
    }

    pub(crate) fn peer_valid_for(
        &self,
        server_ip: Option<Ipv4Addr>,
        not_before: OffsetDateTime,
        lifetime: Duration,
    ) -> Peer {
        let id = Uuid7::generate();
        let key = DeviceKey::generate().unwrap();
        let spec = LeafSpec {
            peer_id: id,
            server_ip,
            not_before,
            lifetime,
        };
        let leaf = self.ca.issue(&key.csr_der().unwrap(), &spec).unwrap();
        Peer {
            id,
            identity: key.into_identity(vec![leaf]),
        }
    }
}

pub(crate) async fn listen() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind((LOOPBACK, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

pub(crate) fn target(address: SocketAddr, server_id: Uuid7) -> ServerTarget {
    ServerTarget { address, server_id }
}

pub(crate) fn mutual_server(community: &Community, server: &Peer) -> TlsServer {
    TlsServer::mutual(&server.identity, community.ca_der()).unwrap()
}
