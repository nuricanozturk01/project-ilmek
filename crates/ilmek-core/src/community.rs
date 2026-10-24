//! Community profile and the local device id. Nothing here is secret.

use std::fs;
use std::io;
use std::net::SocketAddrV4;
use std::path::Path;

use ilmek_protocol::Uuid7;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const PROFILE_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("profile is not valid JSON: {0}")]
    Json(String),
    #[error("unsupported profile version {0}")]
    Version(u32),
    #[error("cannot access {path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("{path} does not contain a valid device id")]
    DeviceId { path: String },
}

/// What a device needs to find its community: the community id and the registry address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommunityProfile {
    pub profile_version: u32,
    pub community_id: Uuid7,
    /// Reported by the registry in HELLO_ACK; kept here so a device can recognise it.
    pub registry_id: Uuid7,
    pub registry: SocketAddrV4,
}

impl CommunityProfile {
    #[must_use]
    pub fn new(community_id: Uuid7, registry_id: Uuid7, registry: SocketAddrV4) -> Self {
        Self {
            profile_version: PROFILE_VERSION,
            community_id,
            registry_id,
            registry,
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

    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
        Self::from_json(&bytes)
    }

    pub fn save(&self, path: &Path) -> Result<(), ProfileError> {
        fs::write(path, self.to_json()?).map_err(|source| io_error(path, source))
    }
}

fn io_error(path: &Path, source: io::Error) -> ProfileError {
    ProfileError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Reads the device id stored in `path`, creating a new random one on first use.
pub fn load_or_create_device_id(path: &Path) -> Result<Uuid7, ProfileError> {
    match fs::read_to_string(path) {
        Ok(text) => text.trim().parse().map_err(|_| ProfileError::DeviceId {
            path: path.display().to_string(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
            }
            let id = Uuid7::generate();
            fs::write(path, format!("{id}\n")).map_err(|source| io_error(path, source))?;
            Ok(id)
        }
        Err(source) => Err(io_error(path, source)),
    }
}
