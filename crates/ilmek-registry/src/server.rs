use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use ilmek_core::listener::{ListenerParams, serve_listener};
use ilmek_core::session::ServerSessionConfig;
use ilmek_protocol::wire::ServerRole;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::registry::Registry;
use crate::service::RegistryService;

#[derive(Debug)]
pub struct RunningRegistry {
    pub addr: SocketAddr,
    shutdown: CancellationToken,
    tasks: JoinSet<()>,
}

impl RunningRegistry {
    /// Stops accepting, lets open sessions drain, and returns when everything is down.
    pub async fn shutdown(mut self) {
        self.shutdown.cancel();
        while self.tasks.join_next().await.is_some() {}
    }
}

impl Registry {
    /// Binds the listener and serves until [`RunningRegistry::shutdown`].
    pub async fn serve(self: &Arc<Self>) -> io::Result<RunningRegistry> {
        let shutdown = CancellationToken::new();
        let listener = TcpListener::bind(self.settings.listen).await?;
        let addr = listener.local_addr()?;
        let config = ServerSessionConfig::new(
            self.profile.registry_id,
            ServerRole::Registry,
            self.profile.community_id,
            Arc::clone(&self.clock),
        );
        let params = ListenerParams {
            session: Arc::new(config),
            handler: Arc::new(RegistryService::new(Arc::clone(self))),
            max_connections: self.settings.max_connections,
        };
        let mut tasks = JoinSet::new();
        tasks.spawn(serve_listener(listener, params, shutdown.clone()));
        Ok(RunningRegistry {
            addr,
            shutdown,
            tasks,
        })
    }
}
