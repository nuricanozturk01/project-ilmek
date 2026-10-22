use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ilmek_identity::Identity;
use ilmek_protocol::Uuid7;
use rustls::ClientConfig;
use rustls::client::Resumption;
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use crate::peer::PeerCredentials;

use super::{TlsError, credentials, provider, root_store, verify_session};

const CONNECT_DEADLINE: Duration = Duration::from_secs(5);
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

/// A server the caller already knows how to authenticate (registry record or bootstrap profile).
#[derive(Debug, Clone, Copy)]
pub struct ServerTarget {
    pub address: SocketAddr,
    pub server_id: Uuid7,
}

pub struct TlsClient {
    connector: TlsConnector,
}

impl std::fmt::Debug for TlsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsClient").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct ClientConnection {
    pub stream: tokio_rustls::client::TlsStream<TcpStream>,
    pub server: PeerCredentials,
}

impl TlsClient {
    /// `identity` is `None` only for enrollment, before the device has a certificate.
    pub fn new(identity: Option<&Identity>, ca_der: &[u8]) -> Result<Self, TlsError> {
        let builder = ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_root_certificates(root_store(ca_der)?);
        let mut config = match identity {
            Some(identity) => {
                let (chain, key) = credentials(identity);
                builder.with_client_auth_cert(chain, key)?
            }
            None => builder.with_no_client_auth(),
        };
        config.alpn_protocols = vec![super::ALPN.to_vec()];
        config.resumption = Resumption::disabled();
        Ok(Self {
            connector: TlsConnector::from(Arc::new(config)),
        })
    }

    pub async fn connect(&self, target: ServerTarget) -> Result<ClientConnection, TlsError> {
        let tcp = timeout(CONNECT_DEADLINE, TcpStream::connect(target.address))
            .await
            .map_err(|_| TlsError::ConnectTimeout)?
            .map_err(TlsError::Connect)?;
        self.handshake(tcp, target).await
    }

    /// Runs the TLS handshake over an already connected socket.
    pub async fn handshake(
        &self,
        tcp: TcpStream,
        target: ServerTarget,
    ) -> Result<ClientConnection, TlsError> {
        let name = ServerName::IpAddress(target.address.ip().into());
        let stream = timeout(HANDSHAKE_DEADLINE, self.connector.connect(name, tcp))
            .await
            .map_err(|_| TlsError::HandshakeTimeout)?
            .map_err(TlsError::from_handshake)?;

        let server = verify_session(stream.get_ref().1)?.ok_or(TlsError::MissingPeerCertificate)?;
        if server.id != target.server_id {
            return Err(TlsError::UnexpectedPeer {
                expected: target.server_id,
                actual: server.id,
            });
        }
        Ok(ClientConnection { stream, server })
    }
}
