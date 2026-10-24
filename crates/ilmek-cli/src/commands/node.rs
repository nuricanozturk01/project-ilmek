use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ilmek_client::Device;
use ilmek_core::clock::{Clock, SystemClock};
use ilmek_core::community::CommunityProfile;
use ilmek_core::discovery::serve_beacon;
use ilmek_host::{Host, ProcessLauncher, Settings as HostSettings};
use ilmek_registry::{InitOptions, Registry, Settings as RegistrySettings, initialize};
use tokio_util::sync::CancellationToken;

use super::{discovery_port, find_profile, home, host_config, registry_config};
use crate::cli::{Host as HostCommand, Node, Registry as RegistryCommand};
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

fn write_if_missing(path: &Path, text: &str) -> CliResult {
    if !path.exists() {
        std::fs::write(path, text).context("cannot write a configuration file")?;
    }
    Ok(())
}

/// TOML strings: backslashes (Windows paths) must not be read as escapes.
fn literal(path: &Path) -> String {
    format!("'{}'", path.display())
}

/// Writes `registry.toml` and `host.toml` the first time; later runs keep whatever is there.
fn write_configs(home: &Path, args: &Node) -> CliResult<(PathBuf, PathBuf)> {
    let bind = if args.local_only {
        "127.0.0.1"
    } else {
        "0.0.0.0"
    };

    let registry_toml = home.join("registry.toml");
    write_if_missing(
        &registry_toml,
        &format!(
            "data_dir = {}\nlisten = \"{bind}:{}\"\n",
            literal(&home.join("registry")),
            args.registry_port
        ),
    )?;
    let host_toml = home.join("host.toml");
    write_if_missing(
        &host_toml,
        &format!(
            "data_dir = {}\ndevice_dir = {}\nlisten = \"{bind}:{}\"\n",
            literal(&home.join("host")),
            literal(&home.join("host-device")),
            args.host_port
        ),
    )?;
    Ok((registry_toml, host_toml))
}

pub(crate) async fn node(args: Node) -> CliResult {
    let home = home(args.home.as_deref());
    std::fs::create_dir_all(&home).context("cannot create the data directory")?;
    let (registry_toml, host_toml) = write_configs(&home, &args)?;

    // Registry: the first run creates the community.
    let registry_settings =
        RegistrySettings::load(&registry_toml).context("registry configuration")?;
    let profile_path = registry_settings.data_dir.join("community.json");
    if !profile_path.exists() {
        initialize(
            &registry_settings,
            &InitOptions {
                advertised_ip: Ipv4Addr::LOCALHOST,
            },
            SystemClock.now_ms(),
        )
        .context("cannot create the community")?;
        note("created a new community");
    }
    let profile = CommunityProfile::load(&profile_path).context("community profile")?;
    let registry = Registry::open(registry_settings, Arc::new(SystemClock))
        .context("cannot open the registry")?;
    let running_registry = registry
        .serve()
        .await
        .context("cannot start the registry (is the port taken? try --registry-port)")?;

    // The port the registry really listens on wins over what an earlier run stored in the
    // profile: it is what the beacon announces and what this machine's own host connects to.
    let live = CommunityProfile::new(
        profile.community_id,
        profile.registry_id,
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, running_registry.addr.port()),
    );

    // This machine's own application host joins by itself.
    let host_settings = HostSettings::load(&host_toml).context("host configuration")?;
    Device::join(&live, &host_settings.device_dir).context("cannot join the host")?;
    let host = Host::open(
        host_settings,
        Arc::new(ProcessLauncher),
        Arc::new(SystemClock),
    )
    .await
    .context("cannot open the host")?;
    let running_host = host
        .serve()
        .await
        .context("cannot start the host (is the port taken? try --host-port)")?;

    // So that other devices can find this node without being told its address.
    let beacon_stop = CancellationToken::new();
    let beacon = tokio::spawn(serve_beacon(
        live,
        discovery_port(args.discovery_port),
        beacon_stop.clone(),
    ));

    note(&format!(
        "node is running: registry {}, application host {}",
        running_registry.addr, running_host.local_addr
    ));
    note(&format!("data: {}", home.display()));
    if args.local_only {
        note("local-only: other devices cannot connect");
    } else {
        note(&format!(
            "other devices on this network: run `ilmek join` (finds this node by itself), or \
             `ilmek join --registry <this machine's IP>:{}`",
            args.registry_port
        ));
    }
    note(
        "publish an app: ilmek app pack <dir> --out pkg, then ilmek app install pkg, ilmek app start <ns>/<slug>, ilmek app publish <ns>/<slug>",
    );

    shutdown_signal().await;
    note("shutting down");
    beacon_stop.cancel();
    match beacon.await {
        Ok(Ok(())) | Err(_) => {}
        Ok(Err(error)) => note(&format!(
            "discovery was not available ({error}); devices must use --registry"
        )),
    }
    running_host.shutdown().await;
    running_registry.shutdown().await;
    Ok(())
}

pub(crate) async fn registry(command: RegistryCommand) -> CliResult {
    let RegistryCommand::Serve {
        registry,
        advertise,
    } = command;
    let settings =
        RegistrySettings::load(&registry_config(&registry)).context("registry configuration")?;
    let profile_path = settings.data_dir.join("community.json");
    if !profile_path.exists() {
        let advertised_ip = match (advertise, settings.listen.ip()) {
            (Some(ip), _) => ip,
            (None, std::net::IpAddr::V4(ip)) if !ip.is_unspecified() => ip,
            _ => Ipv4Addr::LOCALHOST,
        };
        initialize(
            &settings,
            &InitOptions { advertised_ip },
            SystemClock.now_ms(),
        )
        .context("cannot create the community")?;
        note(&format!("community created: {}", profile_path.display()));
    }
    let profile = CommunityProfile::load(&profile_path).context("community profile")?;
    let registry =
        Registry::open(settings, Arc::new(SystemClock)).context("cannot open the registry")?;
    let running = registry
        .serve()
        .await
        .context("cannot start the registry")?;
    let live = CommunityProfile::new(
        profile.community_id,
        profile.registry_id,
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, running.addr.port()),
    );
    let beacon_stop = CancellationToken::new();
    let beacon = tokio::spawn(serve_beacon(
        live,
        discovery_port(None),
        beacon_stop.clone(),
    ));
    note(&format!("registry listening on {}", running.addr));
    shutdown_signal().await;
    note("shutting down");
    beacon_stop.cancel();
    let _ = beacon.await;
    running.shutdown().await;
    Ok(())
}

pub(crate) async fn host(command: HostCommand) -> CliResult {
    match command {
        HostCommand::Serve { host } => {
            let settings = HostSettings::load(&host_config(&host)).context("host configuration")?;
            serve_host(settings).await
        }
        HostCommand::Start {
            home: explicit,
            registry,
            port,
        } => {
            let home = home(explicit.as_deref());
            std::fs::create_dir_all(&home).context("cannot create the data directory")?;
            let host_toml = home.join("host.toml");
            write_if_missing(
                &host_toml,
                &format!(
                    "data_dir = {}\ndevice_dir = {}\nlisten = \"0.0.0.0:{port}\"\n",
                    literal(&home.join("host")),
                    literal(&home.join("host-device")),
                ),
            )?;
            let settings = HostSettings::load(&host_toml).context("host configuration")?;
            if !Device::exists(&settings.device_dir) {
                let profile = find_profile(registry).await?;
                Device::join(&profile, &settings.device_dir).context("cannot join")?;
                note(&format!("joined community {}", profile.community_id));
            }
            serve_host(settings).await
        }
    }
}

async fn serve_host(settings: HostSettings) -> CliResult {
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
