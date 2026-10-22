//! The community registry: enrollment CA, name ownership, host leases and trust snapshots.

mod admin;
mod catalog;
mod enroll;
mod error;
mod files;
mod gate;
mod init;
mod limiter;
mod operations;
mod peers;
mod registry;
mod schema;
mod server;
mod service;
mod settings;
mod snapshots;
mod time_util;
mod trust_api;

pub use admin::{InviteRequest, PeerSummary};
pub use error::RegistryError;
pub use gate::RegistryGate;
pub use init::{InitOptions, initialize, reissue_identity};
pub use registry::Registry;
pub use server::RunningRegistry;
pub use service::RegistryService;
pub use settings::{Settings, SettingsError};
