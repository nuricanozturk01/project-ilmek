//! Ed25519 publisher keys and manifest signatures (PROTOCOL.md §15).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use ilmek_protocol::digest::{hex_lower, manifest_signing_preimage, sha256};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PublisherError {
    #[error("public key must be 32 base64url bytes of a valid Ed25519 point")]
    PublicKey,
    #[error("signature must be 64 bytes")]
    SignatureLength,
    #[error("manifest signature does not verify")]
    BadSignature,
    #[error("system randomness unavailable")]
    Randomness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublisherKey(VerifyingKey);

impl PublisherKey {
    pub fn from_base64url(encoded: &str) -> Result<Self, PublisherError> {
        let raw: [u8; 32] = URL_SAFE_NO_PAD
            .decode(encoded)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(PublisherError::PublicKey)?;
        VerifyingKey::from_bytes(&raw)
            .map(Self)
            .map_err(|_| PublisherError::PublicKey)
    }

    #[must_use]
    pub fn to_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0.as_bytes())
    }

    /// Lowercase hex SHA-256 of the raw public key.
    #[must_use]
    pub fn key_id(&self) -> String {
        hex_lower(&sha256(self.0.as_bytes()))
    }

    /// Verifies a signature over the exact stored `manifest.json` bytes.
    pub fn verify_manifest(
        &self,
        manifest_bytes: &[u8],
        signature: &[u8],
    ) -> Result<(), PublisherError> {
        let signature =
            Signature::from_slice(signature).map_err(|_| PublisherError::SignatureLength)?;
        self.0
            .verify_strict(&manifest_signing_preimage(manifest_bytes), &signature)
            .map_err(|_| PublisherError::BadSignature)
    }
}

pub struct PublisherSigner(SigningKey);

impl std::fmt::Debug for PublisherSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublisherSigner").finish_non_exhaustive()
    }
}

impl PublisherSigner {
    pub fn generate() -> Result<Self, PublisherError> {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|_| PublisherError::Randomness)?;
        Ok(Self::from_seed(&seed))
    }

    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(seed))
    }

    #[must_use]
    pub fn public_key(&self) -> PublisherKey {
        PublisherKey(self.0.verifying_key())
    }

    #[must_use]
    pub fn sign_manifest(&self, manifest_bytes: &[u8]) -> [u8; 64] {
        self.0
            .sign(&manifest_signing_preimage(manifest_bytes))
            .to_bytes()
    }
}
