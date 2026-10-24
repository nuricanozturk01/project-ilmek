use std::path::{Path, PathBuf};
use std::sync::Arc;

use ilmek_client::{
    AppClient, Connector, Device, PackageCache, Progress, RegistryClient, Resolver, Source,
    fetch_package,
};
use ilmek_core::clock::{Clock, SystemClock};
use ilmek_core::community::CommunityProfile;
use ilmek_core::discovery::{DISCOVERY_PORT, discover_at, discover_on};
use ilmek_protocol::address::Address;
use ilmek_protocol::wire::{Availability, Cursor};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::RuntimeError;
use crate::events::EventSink;
use crate::session::{AppSession, SessionParts};
use crate::storage::AppStorage;

/// The UDP port nodes answer on: 47400, or `$ILMEK_DISCOVERY_PORT`.
fn discovery_port() -> u16 {
    std::env::var("ILMEK_DISCOVERY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DISCOVERY_PORT)
}

/// Highest package runtime version this client understands.
const RUNTIME_VERSION: ilmek_protocol::wire::Version = ilmek_protocol::wire::Version::new(1, 0, 0);
const DEFAULT_CACHE_QUOTA: u64 = 512 * 1024 * 1024;
const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub community_id: String,
    pub peer_id: String,
    pub registry: String,
    pub registry_reachable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub address: String,
    pub name: String,
    pub description: String,
    pub publisher_id: String,
    pub version: String,
    pub availability: &'static str,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogPage {
    pub entries: Vec<CatalogEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedApp {
    pub address: String,
    pub version: String,
    pub bytes: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct History {
    visited: Vec<String>,
    bookmarks: Vec<String>,
}

/// The user-facing facade over joining, discovery, downloading and opening applications.
pub struct Client {
    root: PathBuf,
    connector: Arc<Connector>,
    resolver: Resolver,
    cache: PackageCache,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Client {
    #[must_use]
    pub fn is_joined(root: &Path) -> bool {
        Device::exists(&root.join("device"))
    }

    /// Joins a community from the profile file its registry wrote.
    pub fn join(root: &Path, profile_json: &[u8]) -> Result<(), RuntimeError> {
        let profile =
            CommunityProfile::from_json(profile_json).map_err(ilmek_client::Error::from)?;
        Device::join(&profile, &root.join("device"))?;
        Ok(())
    }

    /// Joins by looking for a node on the local network (or at `registry`, if given).
    pub async fn join_auto(
        root: &Path,
        registry: Option<std::net::SocketAddrV4>,
    ) -> Result<(), RuntimeError> {
        let wait = std::time::Duration::from_secs(4);
        let port = discovery_port();
        let profile = match registry {
            Some(address) => {
                let mut found = discover_at(*address.ip(), port, wait)
                    .await
                    .map_err(ilmek_client::Error::from)?;
                found.registry = address;
                found
            }
            None => discover_on(port, wait)
                .await
                .map_err(ilmek_client::Error::from)?,
        };
        Device::join(&profile, &root.join("device"))?;
        Ok(())
    }

    pub fn open(root: &Path) -> Result<Self, RuntimeError> {
        Self::open_with_clock(root, Arc::new(SystemClock))
    }

    pub fn open_with_clock(root: &Path, clock: Arc<dyn Clock>) -> Result<Self, RuntimeError> {
        let device = Device::load(&root.join("device"))?;
        let connector = Arc::new(Connector::new(device));
        let resolver = Resolver::new(Arc::clone(&connector), clock, Some(root.join("names.json")));
        let cache = PackageCache::new(root.join("cache"), DEFAULT_CACHE_QUOTA);
        Ok(Self {
            root: root.to_path_buf(),
            connector,
            resolver,
            cache,
        })
    }

    #[must_use]
    pub fn connector(&self) -> &Arc<Connector> {
        &self.connector
    }

    /// Contacts the registry and returns the resulting status.
    pub async fn refresh(&self) -> Status {
        let reachable = match RegistryClient::connect(&self.connector).await {
            Ok(registry) => {
                registry.close().await;
                true
            }
            Err(_) => false,
        };
        self.status(reachable)
    }

    fn status(&self, registry_reachable: bool) -> Status {
        let device = self.connector.device();
        Status {
            community_id: device.profile.community_id.to_string(),
            peer_id: device.peer_id.to_string(),
            registry: device.profile.registry.to_string(),
            registry_reachable,
        }
    }

    /// Lists published applications, 25 at a time.
    pub async fn catalog(
        &self,
        query: &str,
        cursor: Option<String>,
    ) -> Result<CatalogPage, RuntimeError> {
        let cursor = cursor
            .map(|text| text.parse::<Cursor>())
            .transpose()
            .map_err(|_| RuntimeError::Invalid("cursor is not valid"))?;
        let registry = RegistryClient::connect(&self.connector).await?;
        let (meta, page) = registry.list_apps(query, 25, cursor).await?;
        registry.close().await;
        let entries = page
            .records
            .into_iter()
            .map(|record| CatalogEntry {
                cached: self.cache.lookup(&record).ok().flatten().is_some(),
                address: format!("ilmek://{}/{}", record.namespace, record.slug),
                name: record.name.to_string(),
                description: record.description.to_string(),
                publisher_id: record.publisher_id.to_string(),
                version: record.version.to_string(),
                availability: match record.availability {
                    Availability::RecentHeartbeat => "recent_heartbeat",
                    Availability::Stale => "stale",
                    Availability::Unknown => "unknown",
                },
            })
            .collect();
        Ok(CatalogPage {
            entries,
            next_cursor: meta.next_cursor.map(|c| c.to_string()),
        })
    }

    /// Resolves, downloads (resuming and verifying) and opens an application.
    pub async fn open_app(
        &self,
        address: &str,
        sink: Arc<dyn EventSink>,
        on_progress: impl FnMut(Progress) + Send,
        cancel: &CancellationToken,
    ) -> Result<(Arc<AppSession>, Source), RuntimeError> {
        let address: Address = address.parse()?;
        let resolution = self.resolver.resolve(&address).await?;
        let record = resolution.record;
        let package =
            fetch_package(&self.connector, &record, &self.cache, on_progress, cancel).await?;

        if package.manifest.min_runtime_version > RUNTIME_VERSION {
            return Err(RuntimeError::Package(
                "this application needs a newer İlmek client".into(),
            ));
        }
        let device = self.connector.device();
        let storage = AppStorage::open(
            &self
                .root
                .join("storage")
                .join(record.community_id.to_string())
                .join(device.peer_id.to_string())
                .join(format!("{}.db", record.app_id)),
        )?;
        let client = AppClient::new(
            Arc::clone(&self.connector),
            record.clone(),
            package.manifest.version,
        );
        self.remember(&address);
        let session = AppSession::new(SessionParts {
            address,
            record,
            manifest: package.manifest,
            package_dir: package.dir,
            client,
            storage,
            sink,
        });
        Ok((session, resolution.source))
    }

    /// Cached packages with their on-disk size.
    #[must_use]
    pub fn cached_apps(&self) -> Vec<CachedApp> {
        let mut found = Vec::new();
        let community_dirs = std::fs::read_dir(self.root.join("cache"))
            .into_iter()
            .flatten()
            .flatten();
        for community in community_dirs {
            for app in std::fs::read_dir(community.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                for version in std::fs::read_dir(app.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let name = version.file_name().to_string_lossy().into_owned();
                    if name.starts_with(".partial") {
                        continue;
                    }
                    // The manifest is only read for display; nothing is trusted from it here.
                    let address = std::fs::read(version.path().join("manifest.json"))
                        .ok()
                        .and_then(|bytes| {
                            ilmek_protocol::manifest::Manifest::parse(
                                &bytes,
                                ilmek_protocol::manifest::DEFAULT_MAX_TOTAL_BYTES,
                            )
                            .ok()
                        })
                        .map_or_else(
                            || app.file_name().to_string_lossy().into_owned(),
                            |m| format!("ilmek://{}/{}", m.namespace, m.slug),
                        );
                    found.push(CachedApp {
                        address,
                        version: name,
                        bytes: dir_size(&version.path()),
                    });
                }
            }
        }
        found
    }

    /// Removes an application's cached package and its local data.
    pub async fn remove_app(&self, address: &str) -> Result<(), RuntimeError> {
        let address: Address = address.parse()?;
        let record = self.resolver.resolve(&address).await?.record;
        self.cache.evict(&record)?;
        let device = self.connector.device();
        let data = self
            .root
            .join("storage")
            .join(record.community_id.to_string())
            .join(device.peer_id.to_string())
            .join(format!("{}.db", record.app_id));
        for suffix in ["", "-wal", "-shm"] {
            let mut file = data.clone().into_os_string();
            file.push(suffix);
            match std::fs::remove_file(PathBuf::from(file)) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Cache plus local data, in bytes.
    #[must_use]
    pub fn storage_usage(&self) -> u64 {
        dir_size(&self.root.join("cache")) + dir_size(&self.root.join("storage"))
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("history.json")
    }

    fn load_history(&self) -> History {
        std::fs::read(self.history_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save_history(&self, history: &History) {
        let Ok(bytes) = serde_json::to_vec(history) else {
            return;
        };
        if let Err(error) = std::fs::write(self.history_path(), bytes) {
            tracing::warn!(%error, "could not save history");
        }
    }

    fn remember(&self, address: &Address) {
        let mut history = self.load_history();
        let text = address.to_string();
        history.visited.retain(|entry| *entry != text);
        history.visited.insert(0, text);
        history.visited.truncate(HISTORY_LIMIT);
        self.save_history(&history);
    }

    #[must_use]
    pub fn visited(&self) -> Vec<String> {
        self.load_history().visited
    }

    #[must_use]
    pub fn bookmarks(&self) -> Vec<String> {
        self.load_history().bookmarks
    }

    pub fn set_bookmark(&self, address: &str, on: bool) -> Result<(), RuntimeError> {
        let address: Address = address.parse()?;
        let text = address.to_string();
        let mut history = self.load_history();
        history.bookmarks.retain(|entry| *entry != text);
        if on {
            history.bookmarks.insert(0, text);
        }
        self.save_history(&history);
        Ok(())
    }

    /// Facts useful for support, deliberately free of keys, tokens and application content.
    #[must_use]
    pub fn diagnostics(&self) -> String {
        let status = self.status(false);
        serde_json::to_string_pretty(&serde_json::json!({
            "community_id": status.community_id,
            "registry": status.registry,
            "cached_apps": self.cached_apps().len(),
            "storage_bytes": self.storage_usage(),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        }))
        .unwrap_or_default()
    }
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}
