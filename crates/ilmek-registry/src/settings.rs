use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid configuration: {0}")]
    Parse(String),
    #[error("invalid allowed endpoint network `{0}` (expected a.b.c.d/len)")]
    Network(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub data_dir: PathBuf,
    pub control_listen: SocketAddr,
    pub enroll_listen: SocketAddr,
    /// Networks hosts may advertise endpoints in; empty means every address is refused.
    pub allowed_endpoints: Vec<String>,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// How often the serving process re-reads revocations written by operator commands.
    #[serde(default = "default_revocation_refresh_ms")]
    pub revocation_refresh_ms: u64,
}

fn default_revocation_refresh_ms() -> u64 {
    2000
}

fn default_max_connections() -> usize {
    ilmek_core::listener::DEFAULT_MAX_CONNECTIONS
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let text = std::fs::read_to_string(path).map_err(|source| SettingsError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, SettingsError> {
        let settings: Self =
            toml::from_str(text).map_err(|e| SettingsError::Parse(e.to_string()))?;
        settings.networks()?;
        if settings.max_connections == 0 || settings.revocation_refresh_ms == 0 {
            return Err(SettingsError::Parse(
                "max_connections and revocation_refresh_ms must be positive".into(),
            ));
        }
        Ok(settings)
    }

    pub(crate) fn networks(&self) -> Result<Vec<Network>, SettingsError> {
        self.allowed_endpoints
            .iter()
            .map(|text| Network::parse(text))
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Network {
    base: u32,
    mask: u32,
}

impl Network {
    fn parse(text: &str) -> Result<Self, SettingsError> {
        let invalid = || SettingsError::Network(text.to_owned());
        let (ip, len) = text.split_once('/').ok_or_else(invalid)?;
        let ip: Ipv4Addr = ip.parse().map_err(|_| invalid())?;
        let len: u32 = len.parse().ok().filter(|l| *l <= 32).ok_or_else(invalid)?;
        let mask = u32::MAX.checked_shl(32 - len).unwrap_or(0);
        Ok(Self {
            base: u32::from(ip) & mask,
            mask,
        })
    }

    pub(crate) fn contains(self, ip: Ipv4Addr) -> bool {
        u32::from(ip) & self.mask == self.base
    }
}
