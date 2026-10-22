//! TLS 1.3 setup shared by clients and servers (PROTOCOL.md §2, §3).
//!
//! Peers are authenticated twice: rustls validates chain, validity period, key usage and
//! (for servers) the IP SAN; then the `urn:uuid:` identity is extracted and compared.

mod client;
mod error;
mod server;

use std::sync::Arc;

use ilmek_identity::{Identity, certificate_serial_hex, peer_id_from_certificate};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{CommonState, RootCertStore};

use crate::peer::PeerCredentials;

pub use client::{ClientConnection, ServerTarget, TlsClient};
pub use error::TlsError;
pub use server::{AcceptedConnection, TlsServer};

/// ALPN identifier: the 7 ASCII bytes `ilmek/1`.
pub const ALPN: &[u8] = b"ilmek/1";

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

fn root_store(ca_der: &[u8]) -> Result<RootCertStore, TlsError> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(ca_der.to_vec()))?;
    Ok(roots)
}

fn credentials(identity: &Identity) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    let chain = identity
        .chain_der
        .iter()
        .cloned()
        .map(CertificateDer::from)
        .collect();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone()));
    (chain, key)
}

/// Checks the negotiated ALPN and returns the peer's certificate credentials, if it sent one.
fn verify_session(session: &CommonState) -> Result<Option<PeerCredentials>, TlsError> {
    if session.alpn_protocol() != Some(ALPN) {
        return Err(TlsError::AlpnMismatch);
    }
    session
        .peer_certificates()
        .and_then(<[CertificateDer<'_>]>::first)
        .map(|leaf| {
            Ok(PeerCredentials {
                id: peer_id_from_certificate(leaf).map_err(TlsError::PeerIdentity)?,
                serial: certificate_serial_hex(leaf).map_err(TlsError::PeerIdentity)?,
            })
        })
        .transpose()
}
