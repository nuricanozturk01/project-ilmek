//! Client side of ILMEK/1: devices, connections and typed service clients.

mod app;
mod connector;
mod device;
mod download;
mod error;
mod registry_client;
mod resolver;
mod retry;

pub use app::{AppClient, CallResult, Delivered, EventSubscription};
pub use connector::Connector;
pub use device::Device;
pub use download::{CachedPackage, PackageCache, Progress, fetch_package};
pub use error::Error;
pub use registry_client::RegistryClient;
pub use resolver::{Resolution, Resolver, Source};
pub use retry::{RetryPolicy, retry_write};
