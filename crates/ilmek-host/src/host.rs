use std::io;
use std::sync::Arc;

use ilmek_client::{Connector, Device};
use ilmek_core::clock::Clock;
use ilmek_core::listener::{ListenerParams, serve_listener};
use ilmek_core::session::{PeerGate, ServerSessionConfig};
use ilmek_core::tls::TlsServer;
use ilmek_protocol::wire::{Endpoint, PeerRole, ServerRole};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::error::HostError;
use crate::install::Installer;
use crate::launcher::Launcher;
use crate::manager::Manager;
use crate::registry_link;
use crate::service::HostService;
use crate::settings::Settings;
use crate::store::Store;
use crate::supervisor::Env;

/// A configured host: database, packages, supervised backends and its network identity.
pub struct Host {
    settings: Settings,
    store: Store,
    installer: Installer,
    connector: Arc<Connector>,
    manager: Manager,
    service: HostService,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host")
            .field("peer_id", &self.connector.device().peer_id)
            .finish_non_exhaustive()
    }
}

impl Host {
    pub async fn open(
        settings: Settings,
        launcher: Arc<dyn Launcher>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, HostError> {
        std::fs::create_dir_all(&settings.data_dir)
            .map_err(|e| HostError::io(&settings.data_dir, e))?;
        let store = Store::open(&settings.database_path())?;
        let installer = Installer::new(
            settings.data_dir.clone(),
            store.clone(),
            settings.max_package_bytes,
            Arc::clone(&clock),
        );
        installer.clean_leftovers().await?;

        let device =
            Device::load(&settings.device_dir).map_err(|e| HostError::Device(e.to_string()))?;
        if !device.roles.contains(&PeerRole::Host) {
            return Err(HostError::Device(
                "this device was not enrolled with the host role".into(),
            ));
        }
        let community_id = device.profile.community_id;
        let connector = Arc::new(
            Connector::new(device, Arc::clone(&clock))
                .map_err(|e| HostError::Device(e.to_string()))?,
        );
        let env = Arc::new(Env {
            store: store.clone(),
            settings: settings.clone(),
            launcher,
            clock: Arc::clone(&clock),
            community_id,
        });
        let manager = Manager::start(Arc::clone(&env));
        let service = HostService::new(env, manager.handle());
        Ok(Self {
            settings,
            store,
            installer,
            connector,
            manager,
            service,
            clock,
        })
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    #[must_use]
    pub fn installer(&self) -> &Installer {
        &self.installer
    }

    #[must_use]
    pub fn connector(&self) -> &Arc<Connector> {
        &self.connector
    }

    /// Starts accepting connections and keeps the registry informed until shut down.
    pub async fn serve(self) -> io::Result<RunningHost> {
        let listener = TcpListener::bind(self.settings.listen).await?;
        let local_addr = listener.local_addr()?;
        let device = self.connector.device();
        let tls = TlsServer::mutual(&device.identity, device.profile.ca_der())
            .map_err(|e| io::Error::other(e.to_string()))?;
        let session = Arc::new(ServerSessionConfig::new(
            device.peer_id,
            ServerRole::Host,
            device.profile.community_id,
            Arc::clone(&self.clock),
        ));
        let params = ListenerParams {
            tls,
            session,
            handler: Arc::new(self.service.clone()),
            gate: Arc::clone(self.connector.trust()) as Arc<dyn PeerGate>,
            max_connections: self.settings.max_connections,
        };

        let shutdown = CancellationToken::new();
        let mut tasks = JoinSet::new();
        tasks.spawn(serve_listener(listener, params, shutdown.clone()));
        let advertised = self.settings.advertised;
        let std::net::IpAddr::V4(ip) = advertised.ip() else {
            return Err(io::Error::other("advertised address must be IPv4"));
        };
        let cadence = registry_link::Cadence {
            heartbeat: std::time::Duration::from_millis(self.settings.heartbeat_interval_ms),
            trust: std::time::Duration::from_millis(self.settings.trust_refresh_ms),
        };
        tasks.spawn(registry_link::maintain(
            Arc::clone(&self.connector),
            self.store.clone(),
            Endpoint {
                ip,
                port: advertised.port(),
            },
            cadence,
            shutdown.clone(),
        ));
        Ok(RunningHost {
            local_addr,
            shutdown,
            tasks,
            manager: self.manager,
        })
    }
}

pub struct RunningHost {
    pub local_addr: std::net::SocketAddr,
    shutdown: CancellationToken,
    tasks: JoinSet<()>,
    manager: Manager,
}

impl std::fmt::Debug for RunningHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningHost")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl RunningHost {
    /// Stops accepting, drains sessions, then stops every backend.
    pub async fn shutdown(mut self) {
        self.shutdown.cancel();
        while self.tasks.join_next().await.is_some() {}
        self.manager.shutdown().await;
    }
}
