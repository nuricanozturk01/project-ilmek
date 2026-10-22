use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    Invalid(String),
}

/// `host.toml`. The advertised address is mandatory so a multi-homed machine never publishes a
/// VPN or internet address by accident.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub data_dir: PathBuf,
    /// Directory holding the enrolled device identity (see `ilmek identity enroll`).
    pub device_dir: PathBuf,
    pub listen: SocketAddr,
    pub advertised: SocketAddr,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_package_limit")]
    pub max_package_bytes: u64,
    #[serde(default = "default_inflight")]
    pub app_max_inflight: usize,
    #[serde(default = "default_queue")]
    pub app_queue_depth: usize,
    #[serde(default = "default_call_deadline_ms")]
    pub call_deadline_ms: u64,
    #[serde(default = "default_start_deadline_ms")]
    pub backend_start_deadline_ms: u64,
    #[serde(default = "default_reconcile_ms")]
    pub reconcile_interval_ms: u64,
    /// How often the lease is renewed at the registry.
    #[serde(default = "default_heartbeat_ms")]
    pub heartbeat_interval_ms: u64,
    /// How often the revocation snapshot is refreshed (the protocol asks for at most 60 s).
    #[serde(default = "default_trust_ms")]
    pub trust_refresh_ms: u64,
}

fn default_heartbeat_ms() -> u64 {
    30_000
}
fn default_trust_ms() -> u64 {
    60_000
}

fn default_max_connections() -> usize {
    ilmek_core::listener::DEFAULT_MAX_CONNECTIONS
}
fn default_package_limit() -> u64 {
    ilmek_protocol::manifest::DEFAULT_MAX_TOTAL_BYTES
}
fn default_inflight() -> usize {
    16
}
fn default_queue() -> usize {
    64
}
fn default_call_deadline_ms() -> u64 {
    12_000
}
fn default_start_deadline_ms() -> u64 {
    10_000
}
fn default_reconcile_ms() -> u64 {
    500
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
            toml::from_str(text).map_err(|e| SettingsError::Invalid(e.to_string()))?;
        let positive = settings.max_connections > 0
            && settings.app_max_inflight > 0
            && settings.call_deadline_ms > 0
            && settings.backend_start_deadline_ms > 0
            && settings.reconcile_interval_ms > 0
            && settings.heartbeat_interval_ms > 0
            && settings.trust_refresh_ms > 0;
        if !positive {
            return Err(SettingsError::Invalid("limits must be positive".into()));
        }
        if !settings.advertised.is_ipv4() || settings.advertised.ip().is_unspecified() {
            return Err(SettingsError::Invalid(
                "advertised must be a concrete IPv4 address:port reachable by clients".into(),
            ));
        }
        Ok(settings)
    }

    #[must_use]
    pub fn call_deadline(&self) -> Duration {
        Duration::from_millis(self.call_deadline_ms)
    }

    #[must_use]
    pub fn start_deadline(&self) -> Duration {
        Duration::from_millis(self.backend_start_deadline_ms)
    }

    #[must_use]
    pub fn reconcile_interval(&self) -> Duration {
        Duration::from_millis(self.reconcile_interval_ms)
    }

    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("host.db")
    }
}
