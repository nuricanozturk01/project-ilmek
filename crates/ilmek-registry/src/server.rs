use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use ilmek_core::listener::{ListenerParams, serve_listener};
use ilmek_core::session::{PeerGate, ServerSessionConfig};
use ilmek_core::tls::TlsServer;
use ilmek_protocol::wire::ServerRole;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::registry::Registry;
use crate::service::RegistryService;

#[derive(Debug)]
pub struct RunningRegistry {
    pub control_addr: SocketAddr,
    pub enroll_addr: SocketAddr,
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
    /// Binds both listeners (control with mTLS, enrollment with server-auth TLS) and serves.
    pub async fn serve(self: &Arc<Self>) -> io::Result<RunningRegistry> {
        let shutdown = CancellationToken::new();
        let gate = self
            .start_gate(shutdown.clone())
            .map_err(|e| io::Error::other(e.to_string()))?;
        let control = TcpListener::bind(self.settings.control_listen).await?;
        let enroll = TcpListener::bind(self.settings.enroll_listen).await?;
        let (control_addr, enroll_addr) = (control.local_addr()?, enroll.local_addr()?);

        let mutual = TlsServer::mutual(&self.identity, self.profile.ca_der())
            .map_err(|e| io::Error::other(e.to_string()))?;
        let enrollment =
            TlsServer::enrollment(&self.identity).map_err(|e| io::Error::other(e.to_string()))?;

        let handler = Arc::new(RegistryService::new(Arc::clone(self)));
        let session = |enrollment_endpoint: bool| {
            let mut config = ServerSessionConfig::new(
                self.profile.registry_id,
                ServerRole::Registry,
                self.profile.community_id,
                Arc::clone(&self.clock),
            );
            config.enrollment_endpoint = enrollment_endpoint;
            Arc::new(config)
        };
        let params = |tls, enrollment_endpoint| ListenerParams {
            tls,
            session: session(enrollment_endpoint),
            handler: Arc::clone(&handler),
            gate: Arc::clone(&gate) as Arc<dyn PeerGate>,
            max_connections: self.settings.max_connections,
        };

        let mut tasks = JoinSet::new();
        tasks.spawn(serve_listener(
            control,
            params(mutual, false),
            shutdown.clone(),
        ));
        tasks.spawn(serve_listener(
            enroll,
            params(enrollment, true),
            shutdown.clone(),
        ));
        Ok(RunningRegistry {
            control_addr,
            enroll_addr,
            shutdown,
            tasks,
        })
    }
}
