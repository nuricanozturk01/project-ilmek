//! Domain-separated hash preimages (PROTOCOL.md §12, §15).

use sha2::{Digest, Sha256};

use crate::ids::Uuid7;

const CALL_DOMAIN: &[u8] = b"ILMEK-CALL-V1\0";

/// Lowercase hex encoding.
#[must_use]
pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

pub type Sha256Digest = [u8; 32];

#[must_use]
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256::digest(bytes).into()
}

/// `SHA256("ILMEK-CALL-V1" 0x00 app_id 0x00 method 0x00 body)` over the exact body bytes.
#[must_use]
pub fn payload_hash(app_id: Uuid7, method: &str, body: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(CALL_DOMAIN);
    hasher.update(app_id.to_string());
    hasher.update([0]);
    hasher.update(method);
    hasher.update([0]);
    hasher.update(body);
    hasher.finalize().into()
}
