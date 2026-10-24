use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ilmek",
    version,
    about = "İlmek: kendi ağınızda uygulama yayınlayın ve açın",
    long_about = "İlmek: kendi ağınızda uygulama yayınlayın ve açın.\n\n\
        Bir makinede `ilmek node` çalıştırın (kayıt sunucusu + uygulama sunucusu). Aynı ağdaki \
        diğer makineler kendiliğinden bulur ve katılır; ayrıca bir şey kurmanız gerekmez.\n\n\
        Veriler `$ILMEK_HOME` (varsayılan `~/.ilmek`) altında tutulur."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run this machine as a node: registry and application host in one process. Everything is
    /// created on the first run; other devices on the network find it by themselves.
    Node(Node),
    /// Run only the registry (`ilmek node` already includes it).
    #[command(subcommand)]
    Registry(Registry),
    /// Run an additional application host that joins the node's community by itself.
    #[command(subcommand)]
    Host(Host),
    /// Join a community explicitly (other commands do this automatically when needed).
    Join(Join),
    /// Show this device's id and the registry it uses.
    Whoami(DeviceOpt),
    /// Create, package, install and manage applications.
    #[command(subcommand)]
    App(App),
    /// Resolve an ilmek:// address through the registry.
    Resolve(DeviceAddress),
    /// Step-by-step connectivity check for an ilmek:// address.
    Diagnose(DeviceAddress),
    /// Download and verify an application package into a cache directory.
    Fetch(Fetch),
    /// Call an application method (for testing backends).
    Call(Call),
    /// Back up, verify and restore registry and host data.
    #[command(subcommand)]
    Backup(Backup),
}

/// Where a device keeps its state and which registry to join the first time.
#[derive(Debug, Args)]
pub(crate) struct DeviceOpt {
    /// Device state directory. Default: `<home>/device`.
    #[arg(long)]
    pub(crate) device: Option<PathBuf>,
    /// Registry address (`ip:port`) for the first join. Without it the node is looked up on the
    /// local network.
    #[arg(long)]
    pub(crate) registry: Option<SocketAddr>,
}

#[derive(Debug, Args)]
pub(crate) struct Node {
    /// Data directory. Default: `$ILMEK_HOME` or `~/.ilmek`.
    #[arg(long)]
    pub(crate) home: Option<PathBuf>,
    /// Port of the registry.
    #[arg(long, default_value_t = 47401)]
    pub(crate) registry_port: u16,
    /// Port of the application host.
    #[arg(long, default_value_t = 47402)]
    pub(crate) host_port: u16,
    /// UDP port used so other devices can find this node (default 47400, or
    /// `$ILMEK_DISCOVERY_PORT`).
    #[arg(long)]
    pub(crate) discovery_port: Option<u16>,
    /// Only listen on this machine (no other device can connect).
    #[arg(long)]
    pub(crate) local_only: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Registry {
    /// Run the registry from a `registry.toml`. The first run creates the community.
    Serve {
        #[command(flatten)]
        registry: RegistryConfig,
        /// Address devices reach the registry on. Defaults to the IP in `listen`.
        #[arg(long)]
        advertise: Option<std::net::Ipv4Addr>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum Host {
    /// Start an application host. It looks for the node, joins and registers itself.
    Start {
        /// Data directory. Default: `$ILMEK_HOME` or `~/.ilmek`.
        #[arg(long)]
        home: Option<PathBuf>,
        /// Registry address (`ip:port`). Without it the node is looked up on the local network.
        #[arg(long)]
        registry: Option<SocketAddr>,
        /// Port of this host.
        #[arg(long, default_value_t = 47402)]
        port: u16,
    },
    /// Run a host from a `host.toml` file (see configs/host.toml.example).
    Serve {
        #[command(flatten)]
        host: HostConfig,
    },
}

#[derive(Debug, Args)]
pub(crate) struct Join {
    /// `community.json`, if you have one. Without it the node is looked up on the local network.
    pub(crate) profile: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) device: DeviceOpt,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Backup {
    /// Back up the registry data directory (safe while the registry runs).
    Registry {
        #[command(flatten)]
        registry: RegistryConfig,
        /// New directory to write the backup to; must not exist.
        #[arg(long)]
        out: PathBuf,
    },
    /// Back up the host data directory: host database, installed packages, application data.
    Host {
        #[command(flatten)]
        host: HostConfig,
        /// New directory to write the backup to; must not exist.
        #[arg(long)]
        out: PathBuf,
    },
    /// Check a backup directory without changing anything.
    Verify {
        /// The backup directory.
        #[arg(long)]
        dir: PathBuf,
    },
    /// Restore a registry backup into the configured (new or empty) data directory.
    RestoreRegistry {
        #[command(flatten)]
        registry: RegistryConfig,
        /// The backup directory.
        #[arg(long)]
        from: PathBuf,
    },
    /// Restore a host backup into the configured (new or empty) data directory.
    RestoreHost {
        #[command(flatten)]
        host: HostConfig,
        /// The backup directory.
        #[arg(long)]
        from: PathBuf,
    },
}

#[derive(Debug, Args)]
pub(crate) struct RegistryConfig {
    /// Path to registry.toml. Default: `<home>/registry.toml`.
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct HostConfig {
    /// Path to host.toml. Default: `<home>/host.toml` (written by `ilmek node`).
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum App {
    /// Scaffold an application project.
    Init {
        dir: PathBuf,
        #[arg(long)]
        namespace: String,
        #[arg(long)]
        slug: String,
        /// Defaults to a new random id.
        #[arg(long)]
        publisher_id: Option<String>,
    },
    /// Build a package from a project directory.
    Pack {
        source: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Replace the publisher id from app.toml.
        #[arg(long)]
        publisher_id: Option<String>,
    },
    Install {
        #[command(flatten)]
        host: HostConfig,
        package: PathBuf,
    },
    Update {
        #[command(flatten)]
        host: HostConfig,
        package: PathBuf,
    },
    Rollback {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        #[arg(long)]
        version: String,
    },
    Remove {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        /// Also delete the application's data. Irreversible.
        #[arg(long)]
        purge_data: bool,
    },
    Start {
        #[command(flatten)]
        host: HostConfig,
        app: String,
    },
    Stop {
        #[command(flatten)]
        host: HostConfig,
        app: String,
    },
    Restart {
        #[command(flatten)]
        host: HostConfig,
        app: String,
    },
    List {
        #[command(flatten)]
        host: HostConfig,
    },
    /// Choose the backend executable for an application (a local administrator decision).
    SetBackend {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        #[arg(long)]
        exe: PathBuf,
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Older client versions this backend still serves.
        #[arg(long = "accept-version")]
        accept_versions: Vec<String>,
    },
    /// Register or update the application in the registry.
    Publish {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        #[command(flatten)]
        device: DeviceOpt,
    },
    Unpublish {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        #[command(flatten)]
        device: DeviceOpt,
    },
}

#[derive(Debug, Args)]
pub(crate) struct DeviceAddress {
    pub(crate) address: String,
    #[command(flatten)]
    pub(crate) device: DeviceOpt,
}

#[derive(Debug, Args)]
pub(crate) struct Fetch {
    pub(crate) address: String,
    #[command(flatten)]
    pub(crate) device: DeviceOpt,
    /// Where to put the package. Default: `<home>/cache`.
    #[arg(long)]
    pub(crate) cache: Option<PathBuf>,
    /// Cache quota in MiB.
    #[arg(long, default_value_t = 512)]
    pub(crate) quota_mib: u64,
}

#[derive(Debug, Args)]
pub(crate) struct Call {
    pub(crate) address: String,
    pub(crate) method: String,
    #[command(flatten)]
    pub(crate) device: DeviceOpt,
    /// JSON object body.
    #[arg(long, default_value = "{}")]
    pub(crate) body: String,
    /// The method changes state; an operation id is generated and kept across retries.
    #[arg(long)]
    pub(crate) write: bool,
}
