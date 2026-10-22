mod app;
mod community;
mod device;
mod doctor;
mod identity;
mod publisher;
mod serve;

use std::sync::Arc;

use ilmek_client::{Connector, Device, RegistryClient};
use ilmek_core::clock::SystemClock;
use ilmek_protocol::Uuid7;

use crate::cli::Command;
use crate::error::{CliError, CliResult, Context};

pub(crate) async fn run(command: Command) -> CliResult {
    match command {
        Command::Community(c) => community::run(c),
        Command::Identity(c) => identity::run(c).await,
        Command::Publisher(c) => publisher::run(c),
        Command::Registry(c) => serve::registry(c).await,
        Command::Host(c) => serve::host(c).await,
        Command::App(c) => app::run(c).await,
        Command::Resolve(c) => device::resolve(&c).await,
        Command::Diagnose(c) => device::diagnose(&c).await,
        Command::Fetch(c) => device::fetch(&c).await,
        Command::Call(c) => device::call(&c).await,
        Command::Doctor => {
            doctor::run();
            Ok(())
        }
    }
}

pub(crate) fn uuid(text: &str, what: &str) -> CliResult<Uuid7> {
    text.parse()
        .map_err(|_| CliError::new(format!("{what} is not a UUIDv7: {text}")))
}

/// Loads an enrolled device and brings its trust snapshot up to date.
pub(crate) async fn connect_device(dir: &std::path::Path) -> CliResult<Connector> {
    let device = Device::load(dir).context("cannot load the device identity (enroll first)")?;
    let connector = Connector::new(device, Arc::new(SystemClock)).context("device identity")?;
    let registry = RegistryClient::connect(&connector)
        .await
        .context("cannot reach the registry")?;
    registry
        .refresh_trust(connector.trust())
        .await
        .context("cannot refresh trust information")?;
    registry.close().await;
    Ok(connector)
}
