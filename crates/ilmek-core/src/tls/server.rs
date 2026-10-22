use std::sync::Arc;
use std::time::Duration;

use ilmek_identity::Identity;
use rustls::ServerConfig;
use rustls::server::{NoServerSessionStorage, WebPkiClientVerifier};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;

use crate::peer::PeerCredentials;

use super::{TlsError, credentials, provider, root_store, verify_session};

const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

pub struct TlsServer {
    acceptor: TlsAcceptor,
}

impl std::fmt::Debug for TlsServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsServer").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct AcceptedConnection {
    pub stream: tokio_rustls::server::TlsStream<TcpStream>,
    /// `None` only on the enrollment endpoint, where clients have no certificate yet.
    pub peer: Option<PeerCredentials>,
}

impl TlsServer {
    /// Normal `control`/`bulk` endpoint: clients must present a community certificate.
    pub fn mutual(identity: &Identity, ca_der: &[u8]) -> Result<Self, TlsError> {
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(root_store(ca_der)?), provider())
                .build()?;
        Self::build(identity, Some(verifier))
    }

    /// Enrollment endpoint: server authentication only.
    pub fn enrollment(identity: &Identity) -> Result<Self, TlsError> {
        Self::build(identity, None)
    }

    fn build(
        identity: &Identity,
        client_verifier: Option<Arc<dyn rustls::server::danger::ClientCertVerifier>>,
    ) -> Result<Self, TlsError> {
        let builder = ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?;
        let builder = match client_verifier {
            Some(verifier) => builder.with_client_cert_verifier(verifier),
            None => builder.with_no_client_auth(),
        };
        let (chain, key) = credentials(identity);
        let mut config = builder.with_single_cert(chain, key)?;
        config.alpn_protocols = vec![super::ALPN.to_vec()];
        config.session_storage = Arc::new(NoServerSessionStorage {});
        config.send_tls13_tickets = 0;
        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }

    pub async fn accept(&self, tcp: TcpStream) -> Result<AcceptedConnection, TlsError> {
        let stream = timeout(HANDSHAKE_DEADLINE, self.acceptor.accept(tcp))
            .await
            .map_err(|_| TlsError::HandshakeTimeout)?
            .map_err(TlsError::from_handshake)?;
        let peer = verify_session(stream.get_ref().1)?;
        Ok(AcceptedConnection { stream, peer })
    }
}
