use std::io;

use ilmek_identity::PkiError;
use ilmek_protocol::Uuid7;
use rustls::CertificateError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("tcp connect failed: {0}")]
    Connect(#[source] io::Error),
    #[error("tcp connect timed out")]
    ConnectTimeout,
    #[error("tls handshake timed out")]
    HandshakeTimeout,
    #[error("peer certificate rejected: {0:?}")]
    Certificate(CertificateError),
    #[error("tls handshake failed: {0}")]
    Handshake(#[source] io::Error),
    #[error("peer did not negotiate ALPN `ilmek/1`")]
    AlpnMismatch,
    #[error("peer certificate has no usable identity: {0}")]
    PeerIdentity(#[source] PkiError),
    #[error("connected to peer {actual}, expected {expected}")]
    UnexpectedPeer { expected: Uuid7, actual: Uuid7 },
    #[error("peer presented no certificate")]
    MissingPeerCertificate,
    #[error("invalid tls configuration: {0}")]
    Config(#[from] rustls::Error),
    #[error("invalid client verifier: {0}")]
    Verifier(#[from] rustls::server::VerifierBuilderError),
}

impl TlsError {
    /// Maps a handshake I/O error to a certificate error when rustls reports one.
    pub(super) fn from_handshake(error: io::Error) -> Self {
        let certificate_error = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
            .and_then(|inner| match inner {
                rustls::Error::InvalidCertificate(reason) => Some(reason.clone()),
                _ => None,
            });
        certificate_error.map_or_else(|| Self::Handshake(error), Self::Certificate)
    }
}
