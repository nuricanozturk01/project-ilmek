//! A device's persistent identity: profile, certificate and private key.

use std::path::{Path, PathBuf};

use ilmek_identity::storage::{create_private_dir, write_secret};
use ilmek_identity::{CommunityProfile, Identity};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::PeerRole;
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceRecord {
    peer_id: Uuid7,
    roles: Vec<PeerRole>,
    expires_at_ms: u64,
}

#[derive(Debug)]
pub struct Device {
    pub profile: CommunityProfile,
    pub peer_id: Uuid7,
    pub roles: Vec<PeerRole>,
    pub expires_at_ms: u64,
    pub identity: Identity,
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

impl Device {
    /// Writes the device to `dir`; the private key is owner-only.
    pub fn save(&self, dir: &Path) -> Result<(), Error> {
        create_private_dir(dir)?;
        let record = DeviceRecord {
            peer_id: self.peer_id,
            roles: self.roles.clone(),
            expires_at_ms: self.expires_at_ms,
        };
        std::fs::write(
            file(dir, "device.json"),
            serde_json::to_vec_pretty(&record).map_err(|_| Error::Corrupt("device record"))?,
        )?;
        std::fs::write(file(dir, "profile.json"), self.profile.to_json()?)?;
        let chain: Vec<u8> = self
            .identity
            .chain_der
            .iter()
            .flat_map(|der| {
                let length = u32::try_from(der.len()).unwrap_or(u32::MAX);
                length
                    .to_be_bytes()
                    .into_iter()
                    .chain(der.iter().copied())
                    .collect::<Vec<_>>()
            })
            .collect();
        std::fs::write(file(dir, "chain.bin"), chain)?;
        let key_path = file(dir, "device.key.der");
        match std::fs::remove_file(&key_path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        write_secret(&key_path, &self.identity.key_pkcs8_der)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self, Error> {
        let record: DeviceRecord =
            serde_json::from_slice(&std::fs::read(file(dir, "device.json"))?)
                .map_err(|_| Error::Corrupt("device.json"))?;
        let profile = CommunityProfile::from_json(&std::fs::read(file(dir, "profile.json"))?)?;
        let chain_der = split_chain(&std::fs::read(file(dir, "chain.bin"))?)?;
        let key_pkcs8_der = std::fs::read(file(dir, "device.key.der"))?;
        Ok(Self {
            profile,
            peer_id: record.peer_id,
            roles: record.roles,
            expires_at_ms: record.expires_at_ms,
            identity: Identity {
                chain_der,
                key_pkcs8_der,
            },
        })
    }

    #[must_use]
    pub fn exists(dir: &Path) -> bool {
        file(dir, "device.json").exists()
    }
}

fn split_chain(mut bytes: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let mut chain = Vec::new();
    while !bytes.is_empty() {
        let (length, rest) = bytes
            .split_first_chunk::<4>()
            .ok_or(Error::Corrupt("chain.bin"))?;
        let length = u32::from_be_bytes(*length) as usize;
        if rest.len() < length {
            return Err(Error::Corrupt("chain.bin"));
        }
        let (certificate, rest) = rest.split_at(length);
        chain.push(certificate.to_vec());
        bytes = rest;
    }
    if chain.is_empty() {
        return Err(Error::Corrupt("chain.bin"));
    }
    Ok(chain)
}
