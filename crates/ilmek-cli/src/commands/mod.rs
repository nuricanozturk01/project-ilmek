mod app;
mod backup;
mod device;
mod node;

use std::path::{Path, PathBuf};
use std::time::Duration;

use ilmek_client::{Connector, Device};
use ilmek_core::community::CommunityProfile;
use ilmek_core::discovery::{DEFAULT_REGISTRY_PORT, DISCOVERY_PORT, discover_any, probe_tcp};
use ilmek_protocol::Uuid7;

use crate::cli::{Command, DeviceOpt, HostConfig, RegistryConfig};
use crate::error::{CliError, CliResult, Context};
use crate::output::note;

const DISCOVERY_WAIT: Duration = Duration::from_secs(4);

/// 47400, or `$ILMEK_DISCOVERY_PORT` (every device of one network must use the same value).
pub(crate) fn discovery_port(explicit: Option<u16>) -> u16 {
    explicit
        .or_else(|| {
            std::env::var("ILMEK_DISCOVERY_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(DISCOVERY_PORT)
}

pub(crate) async fn run(command: Command) -> CliResult {
    match command {
        Command::Node(args) => node::node(args).await,
        Command::Registry(command) => node::registry(command).await,
        Command::Host(command) => node::host(command).await,
        Command::Join(args) => device::join(&args).await,
        Command::Whoami(opt) => device::whoami(&opt),
        Command::App(c) => app::run(c).await,
        Command::Resolve(c) => device::resolve(&c).await,
        Command::Diagnose(c) => device::diagnose(&c).await,
        Command::Fetch(c) => device::fetch(&c).await,
        Command::Call(c) => device::call(&c).await,
        Command::Backup(c) => backup::run(&c),
    }
}

pub(crate) fn uuid(text: &str, what: &str) -> CliResult<Uuid7> {
    text.parse()
        .map_err(|_| CliError::new(format!("{what} is not a UUIDv7: {text}")))
}

/// `$ILMEK_HOME`, else `~/.ilmek`, else `./.ilmek`.
pub(crate) fn home(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Some(path) = std::env::var_os("ILMEK_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(path);
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(
            || PathBuf::from(".ilmek"),
            |h| PathBuf::from(h).join(".ilmek"),
        )
}

pub(crate) fn host_config(config: &HostConfig) -> PathBuf {
    config
        .config
        .clone()
        .unwrap_or_else(|| home(None).join("host.toml"))
}

pub(crate) fn registry_config(config: &RegistryConfig) -> PathBuf {
    config
        .config
        .clone()
        .unwrap_or_else(|| home(None).join("registry.toml"))
}

pub(crate) fn device_dir(opt: &DeviceOpt) -> PathBuf {
    opt.device
        .clone()
        .unwrap_or_else(|| home(None).join("device"))
}

/// The registry's TCP port a network scan tries: 47401, or `$ILMEK_REGISTRY_PORT`.
pub(crate) fn scan_port() -> u16 {
    std::env::var("ILMEK_REGISTRY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_REGISTRY_PORT)
}

/// Finds the node: at `registry` if given (over TCP only), otherwise anywhere on the local
/// network (UDP beacon and a TCP scan of the local /24, whichever answers first).
pub(crate) async fn find_profile(
    registry: Option<std::net::SocketAddr>,
) -> CliResult<CommunityProfile> {
    match registry {
        Some(std::net::SocketAddr::V4(address)) => probe_tcp(address, DISCOVERY_WAIT).await.context(
            "cannot reach the node at that address (is `ilmek node` running, and is its TCP port open in the firewall?)",
        ),
        Some(std::net::SocketAddr::V6(_)) => Err(CliError::new("only IPv4 is supported")),
        None => {
            note("looking for an İlmek node on this network…");
            discover_any(discovery_port(None), scan_port(), DISCOVERY_WAIT)
                .await
                .context(
                    "no node found (start `ilmek node` on one machine, open TCP 47401-47402 in its firewall, or pass --registry IP:PORT)",
                )
        }
    }
}

/// Opens this device, joining the community first if that has not happened yet.
pub(crate) async fn connect_device(opt: &DeviceOpt) -> CliResult<Connector> {
    let dir = device_dir(opt);
    if !Device::exists(&dir) {
        let profile = find_profile(opt.registry).await?;
        Device::join(&profile, &dir).context("cannot join")?;
        note(&format!("joined community {}", profile.community_id));
    }
    let device = Device::load(&dir).context("cannot load the device")?;
    Ok(Connector::new(device))
}
