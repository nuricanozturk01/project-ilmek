//! The community registry: name ownership and host leases. No authentication.

mod catalog;
mod error;
mod files;
mod init;
mod operations;
mod registry;
mod schema;
mod server;
mod service;
mod settings;
mod snapshots;

pub use error::RegistryError;
pub use init::{InitOptions, initialize};
pub use registry::Registry;
pub use server::RunningRegistry;
pub use service::RegistryService;
pub use settings::{Settings, SettingsError};
