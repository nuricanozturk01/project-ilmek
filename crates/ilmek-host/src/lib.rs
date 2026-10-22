//! The multi-application host.

mod error;
mod host;
mod install;
mod launcher;
mod link;
mod logs;
mod manager;
mod paths;
mod publish;
mod registry_link;
mod schema;
mod service;
mod settings;
mod slots;
mod store;
mod subscriptions;
mod supervisor;

pub use error::HostError;
pub use host::{Host, RunningHost};
pub use install::{Installed, Installer, RUNTIME_VERSION};
pub use launcher::{ExitReport, Killer, LaunchSpec, Launcher, ProcessLauncher, RunningBackend};
pub use link::{BackendLink, IpcSubscription, LinkError};
pub use manager::Manager;
pub use publish::{PublishError, publish, unpublish};
pub use service::HostService;
pub use settings::{Settings, SettingsError};
pub use slots::{AppSlot, CallGate};
pub use store::{AppRow, AppState, BackendRow, Desired, StatusRow, Store, VersionRow};
