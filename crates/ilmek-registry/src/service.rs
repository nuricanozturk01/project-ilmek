use std::sync::Arc;

use ilmek_core::session::{Handler, Reply};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{ErrorData, Message, Payload};

use crate::error::RegistryError;
use crate::registry::Registry;

/// Binds the registry to the session layer.
#[derive(Debug, Clone)]
pub struct RegistryService {
    registry: Arc<Registry>,
}

impl RegistryService {
    #[must_use]
    pub fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }
}

fn error_message(error: &RegistryError) -> Message {
    Message::Error(ErrorData {
        code: error.code(),
        message: error.public_message().parse().unwrap_or_default(),
        retryable: error.retryable(),
        retry_after_ms: error.retry_after_ms(),
        details: serde_json::Map::new(),
    })
}

fn json_body<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, RegistryError> {
    serde_json::to_vec(value).map_err(|_| RegistryError::Internal("body serialization".into()))
}

impl Handler for RegistryService {
    async fn handle(&self, request: Message, reply: Reply) {
        let registry = &self.registry;
        let remote = reply.session().remote_ip;
        let Some(peer) = reply.session().peer.as_ref().map(|p| p.id) else {
            // Enrollment connections carry no certificate; only ENROLL is reachable there.
            let outcome = match request {
                Message::Enroll(enroll) => {
                    registry.enroll(remote, enroll).await.map(Message::Enrolled)
                }
                _ => Err(RegistryError::Invalid(
                    "message not valid on this connection",
                )),
            };
            return finish(&reply, outcome).await;
        };
        let outcome = dispatch(registry, peer, request).await;
        finish(&reply, outcome).await;
    }
}

async fn finish(reply: &Reply, outcome: Result<Message, RegistryError>) {
    let message = outcome.unwrap_or_else(|error| {
        if matches!(error, RegistryError::Store(_) | RegistryError::Internal(_)) {
            tracing::error!(error = ?error, "registry request failed internally");
        }
        error_message(&error)
    });
    let _ = reply.finish(&message).await;
}

async fn dispatch(
    registry: &Arc<Registry>,
    peer: Uuid7,
    request: Message,
) -> Result<Message, RegistryError> {
    Ok(match request {
        Message::Resolve(r) => Message::Resolved(registry.resolve(peer, r).await?),
        Message::ListApps(r) => {
            let (data, page) = registry.list_apps(peer, r).await?;
            Message::AppsListed(Payload {
                data,
                body: json_body(&page)?,
            })
        }
        Message::HostRegister(r) => Message::HostRegistered(registry.host_register(peer, r).await?),
        Message::HostHeartbeat(r) => Message::HeartbeatAck(registry.host_heartbeat(peer, r).await?),
        Message::RegisterApp(r) => Message::AppRegistered(registry.register_app(peer, r).await?),
        Message::UpdateApp(r) => Message::AppUpdated(registry.update_app(peer, r).await?),
        Message::UnregisterApp(r) => {
            Message::AppUnregistered(registry.unregister_app(peer, r).await?)
        }
        Message::GetTrust(r) => {
            let (data, page) = registry.get_trust(peer, r).await?;
            Message::TrustPage(Payload {
                data,
                body: json_body(&page)?,
            })
        }
        _ => return Err(RegistryError::Invalid("message not served by the registry")),
    })
}
