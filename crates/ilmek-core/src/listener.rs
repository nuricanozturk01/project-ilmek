//! TCP accept loop: TLS, connection cap and graceful shutdown (PROTOCOL.md §7).

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::session::{ConnectionParams, Handler, PeerGate, ServerSessionConfig, serve_connection};
use crate::tls::TlsServer;

/// Reference limit for concurrent TLS connections on one listener.
pub const DEFAULT_MAX_CONNECTIONS: usize = 128;

pub struct ListenerParams<H> {
    pub tls: TlsServer,
    pub session: Arc<ServerSessionConfig>,
    pub handler: Arc<H>,
    pub gate: Arc<dyn PeerGate>,
    pub max_connections: usize,
}

impl<H> std::fmt::Debug for ListenerParams<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListenerParams").finish_non_exhaustive()
    }
}

/// Accepts connections until `shutdown` fires, then waits for open sessions to finish.
///
/// Beyond `max_connections` a new socket is closed immediately instead of queueing.
pub async fn serve_listener<H: Handler>(
    listener: TcpListener,
    params: ListenerParams<H>,
    shutdown: CancellationToken,
) {
    let tls = Arc::new(params.tls);
    let slots = Arc::new(Semaphore::new(params.max_connections));
    let mut sessions = JoinSet::new();

    loop {
        let (tcp, remote) = tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok(connection) => connection,
                Err(_) => continue,
            },
            Some(_) = sessions.join_next(), if !sessions.is_empty() => continue,
        };
        let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
            drop(tcp);
            continue;
        };
        let tls = Arc::clone(&tls);
        let session = Arc::clone(&params.session);
        let handler = Arc::clone(&params.handler);
        let gate = Arc::clone(&params.gate);
        let shutdown = shutdown.clone();
        sessions.spawn(async move {
            let Ok(accepted) = tls.accept(tcp).await else {
                return;
            };
            let connection = ConnectionParams {
                peer: accepted.peer,
                remote_ip: Some(remote.ip()),
                config: session,
                handler,
                gate,
                shutdown,
            };
            serve_connection(accepted.stream, connection).await;
            drop(slot);
        });
    }
    while sessions.join_next().await.is_some() {}
}
