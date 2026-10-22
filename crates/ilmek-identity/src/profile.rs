//! Bootstrap profile: how a new device finds and authenticates the registry (README §12).

use std::net::SocketAddrV4;

use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::{hex_lower, sha256};
use ilmek_protocol::wire::Token;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("profile is not valid JSON: {0}")]
    Json(String),
    #[error("unsupported profile version {0}")]
    Version(u32),
    #[error("CA fingerprint does not match the one you verified")]
    FingerprintMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommunityProfile {
    pub profile_version: u32,
    pub community_id: Uuid7,
    pub registry_id: Uuid7,
    pub registry_control: SocketAddrV4,
    pub registry_enroll: SocketAddrV4,
    pub ca_certificate: Token,
}

const PROFILE_VERSION: u32 = 1;

impl CommunityProfile {
    #[must_use]
    pub fn new(
        community_id: Uuid7,
        registry_id: Uuid7,
        registry_control: SocketAddrV4,
        registry_enroll: SocketAddrV4,
        ca_certificate_der: Vec<u8>,
    ) -> Self {
        Self {
            profile_version: PROFILE_VERSION,
            community_id,
            registry_id,
            registry_control,
            registry_enroll,
            ca_certificate: Token::new(ca_certificate_der),
        }
    }

    pub fn from_json(text: &[u8]) -> Result<Self, ProfileError> {
        let profile: Self =
            serde_json::from_slice(text).map_err(|e| ProfileError::Json(e.to_string()))?;
        if profile.profile_version != PROFILE_VERSION {
            return Err(ProfileError::Version(profile.profile_version));
        }
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ProfileError> {
        serde_json::to_vec_pretty(self).map_err(|e| ProfileError::Json(e.to_string()))
    }

    #[must_use]
    pub fn ca_der(&self) -> &[u8] {
        self.ca_certificate.as_bytes()
    }

    /// SHA-256 of the CA certificate, lowercase hex; shown to the admin and the new user.
    #[must_use]
    pub fn ca_fingerprint(&self) -> String {
        hex_lower(&sha256(self.ca_der()))
    }

    /// Compares against a fingerprint obtained over a trusted channel (colons and case ignored).
    pub fn verify_fingerprint(&self, expected: &str) -> Result<(), ProfileError> {
        let normalized: String = expected
            .chars()
            .filter(|c| *c != ':' && !c.is_whitespace())
            .collect();
        if normalized.eq_ignore_ascii_case(&self.ca_fingerprint()) {
            Ok(())
        } else {
            Err(ProfileError::FingerprintMismatch)
        }
    }
}
