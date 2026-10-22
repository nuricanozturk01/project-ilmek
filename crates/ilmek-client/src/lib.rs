//! Client side of ILMEK/1: enrolled devices, connections and typed service clients.

mod app;
mod connector;
mod device;
mod download;
mod enroll;
mod error;
mod registry_client;
mod retry;

pub use app::{AppClient, CallResult, Delivered, EventSubscription};
pub use connector::Connector;
pub use device::Device;
pub use download::{CachedPackage, PackageCache, Progress, ensure_not_revoked, fetch_package};
pub use enroll::enroll;
pub use error::Error;
pub use registry_client::RegistryClient;
pub use retry::{RetryPolicy, retry_write};
