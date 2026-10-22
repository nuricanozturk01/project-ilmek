//! The authenticated identity of a TLS peer.

use ilmek_protocol::Uuid7;

/// Taken from the verified certificate, never from message content.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerCredentials {
    pub id: Uuid7,
    /// Lowercase hex certificate serial, matched against revocation lists.
    pub serial: String,
}
