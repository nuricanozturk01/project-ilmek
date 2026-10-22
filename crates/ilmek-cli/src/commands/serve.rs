use std::sync::Arc;

use ilmek_core::clock::SystemClock;
use ilmek_host::{Host, ProcessLauncher, Settings as HostSettings, Store};
use ilmek_identity::PublisherKey;
use ilmek_registry::{Registry, Settings as RegistrySettings};

use crate::cli::{Host as HostCommand, Registry as RegistryCommand};
use crate::error::{CliResult, Context};
use crate::output::note;

/// Resolves on Ctrl-C, and on SIGTERM where available, so services shut down in order.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

pub(crate) async fn registry(command: RegistryCommand) -> CliResult {
    let RegistryCommand::Serve { registry } = command;
    let settings = RegistrySettings::load(&registry.config).context("registry configuration")?;
    let registry =
        Registry::open(settings, Arc::new(SystemClock)).context("cannot open the registry")?;
    let running = registry
        .serve()
        .await
        .context("cannot start the registry")?;
    note(&format!(
        "registry listening: control {}, enrollment {}",
        running.control_addr, running.enroll_addr
    ));
    shutdown_signal().await;
    note("shutting down");
    running.shutdown().await;
    Ok(())
}

pub(crate) async fn host(command: HostCommand) -> CliResult {
    match command {
        HostCommand::Serve { host } => {
            let settings = HostSettings::load(&host.config).context("host configuration")?;
            let opened = Host::open(settings, Arc::new(ProcessLauncher), Arc::new(SystemClock))
                .await
                .context("cannot open the host")?;
            let running = opened.serve().await.context("cannot start the host")?;
            note(&format!("host listening on {}", running.local_addr));
            shutdown_signal().await;
            note("shutting down: stopping backends");
            running.shutdown().await;
            Ok(())
        }
        HostCommand::TrustPublisher { host, key } => {
            let settings = HostSettings::load(&host.config).context("host configuration")?;
            std::fs::create_dir_all(&settings.data_dir)
                .context("cannot create the data directory")?;
            let store =
                Store::open(&settings.database_path()).context("cannot open the host database")?;
            let key = PublisherKey::from_base64url(&key).context("not a valid public key")?;
            let id = store
                .add_trusted_publisher(&key, ilmek_core::clock::Clock::now_ms(&SystemClock))
                .await
                .context("cannot save the key")?;
            note(&format!("trusted publisher key {id}"));
            Ok(())
        }
    }
}
