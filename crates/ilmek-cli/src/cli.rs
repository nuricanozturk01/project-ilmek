use std::net::Ipv4Addr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ilmek",
    version,
    about = "İlmek: internetsiz topluluk uygulama ağı"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create and administer a community (runs on the registry machine).
    #[command(subcommand)]
    Community(Community),
    /// Enroll this device and inspect its identity.
    #[command(subcommand)]
    Identity(Identity),
    /// Publisher signing keys.
    #[command(subcommand)]
    Publisher(Publisher),
    /// Run the registry.
    #[command(subcommand)]
    Registry(Registry),
    /// Run and administer an application host.
    #[command(subcommand)]
    Host(Host),
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
    /// Report what this machine can and cannot do for İlmek (read-only).
    Doctor,
}

#[derive(Debug, Args)]
pub(crate) struct RegistryConfig {
    /// Path to registry.toml.
    #[arg(long)]
    pub(crate) config: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Community {
    /// Create the CA, the registry identity and the bootstrap profile.
    Init {
        #[command(flatten)]
        registry: RegistryConfig,
        /// The address clients reach the registry on (becomes the certificate's IP SAN).
        #[arg(long)]
        advertise_ip: Ipv4Addr,
        /// Registry certificate lifetime in days.
        #[arg(long, default_value_t = 30)]
        identity_days: i64,
    },
    /// Issue a single-use enrollment token (printed to stdout, shown once).
    Invite {
        #[command(flatten)]
        registry: RegistryConfig,
        /// Comma-separated: member, publisher, host, admin.
        #[arg(long, value_delimiter = ',', default_value = "member")]
        role: Vec<String>,
        /// Required for the host role: the address the host certificate must cover.
        #[arg(long)]
        server_ip: Option<Ipv4Addr>,
        /// Renew the identity of this existing peer instead of creating a new one.
        #[arg(long)]
        bound_peer: Option<String>,
        #[arg(long)]
        ttl_seconds: Option<u64>,
    },
    /// List enrolled devices.
    Peers {
        #[command(flatten)]
        registry: RegistryConfig,
    },
    /// Revoke a device certificate.
    RevokePeer {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        peer: String,
    },
    AddPublisher {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        name: String,
    },
    AddPublisherDevice {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        publisher: String,
        #[arg(long)]
        peer: String,
    },
    AddPublisherKey {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        publisher: String,
        /// Base64url public key from `ilmek publisher keygen`.
        #[arg(long)]
        key: String,
    },
    RevokePublisherKey {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        key_id: String,
    },
    GrantNamespace {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        publisher: String,
        #[arg(long)]
        namespace: String,
    },
    GrantHost {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        publisher: String,
        #[arg(long)]
        host: String,
    },
    /// Issue a fresh registry server certificate (the old one expires after 30 days).
    ReissueIdentity {
        #[command(flatten)]
        registry: RegistryConfig,
        #[arg(long)]
        advertise_ip: Ipv4Addr,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum Identity {
    /// Enroll with a token read from stdin (or --token-file). The CA fingerprint is mandatory.
    Enroll {
        #[arg(long)]
        profile: PathBuf,
        /// SHA-256 fingerprint of the CA certificate, obtained from the administrator.
        #[arg(long)]
        fingerprint: String,
        #[arg(long)]
        device: PathBuf,
        #[arg(long, default_value = "ilmek-device")]
        name: String,
        #[arg(long)]
        token_file: Option<PathBuf>,
    },
    Show {
        #[arg(long)]
        device: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum Publisher {
    /// Generate a signing key and write it to an owner-only file.
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    Show {
        #[arg(long)]
        key: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum Registry {
    Serve {
        #[command(flatten)]
        registry: RegistryConfig,
    },
}

#[derive(Debug, Args)]
pub(crate) struct HostConfig {
    /// Path to host.toml.
    #[arg(long)]
    pub(crate) config: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Host {
    Serve {
        #[command(flatten)]
        host: HostConfig,
    },
    /// Trust a publisher key for package installation on this host.
    TrustPublisher {
        #[command(flatten)]
        host: HostConfig,
        #[arg(long)]
        key: String,
    },
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
        #[arg(long)]
        publisher_id: String,
    },
    /// Build and sign a package from a project directory.
    Pack {
        source: PathBuf,
        #[arg(long)]
        key: PathBuf,
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
    /// Register or update the application in the registry, as the publisher.
    Publish {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        /// The publisher's enrolled device directory.
        #[arg(long)]
        device: PathBuf,
    },
    Unpublish {
        #[command(flatten)]
        host: HostConfig,
        app: String,
        #[arg(long)]
        device: PathBuf,
    },
}

#[derive(Debug, Args)]
pub(crate) struct DeviceAddress {
    pub(crate) address: String,
    #[arg(long)]
    pub(crate) device: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct Fetch {
    pub(crate) address: String,
    #[arg(long)]
    pub(crate) device: PathBuf,
    #[arg(long)]
    pub(crate) cache: PathBuf,
    /// Cache quota in MiB.
    #[arg(long, default_value_t = 512)]
    pub(crate) quota_mib: u64,
}

#[derive(Debug, Args)]
pub(crate) struct Call {
    pub(crate) address: String,
    pub(crate) method: String,
    #[arg(long)]
    pub(crate) device: PathBuf,
    /// JSON object body.
    #[arg(long, default_value = "{}")]
    pub(crate) body: String,
    /// The method changes state; an operation id is generated and kept across retries.
    #[arg(long)]
    pub(crate) write: bool,
}
