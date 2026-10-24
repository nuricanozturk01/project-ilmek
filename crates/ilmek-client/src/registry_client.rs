use ilmek_core::session::ClientSession;
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{
    AppRecord, AppUnregistered, AppsListed, AppsPage, Cursor, Endpoint, HeartbeatAck,
    HostHeartbeat, HostRegister, HostRegistered, ListApps, Message, Name, RegisterApp, Resolve,
    UnregisterApp, UpdateApp,
};

use crate::connector::Connector;
use crate::error::Error;

/// Typed calls to the registry over one control connection.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    session: ClientSession,
}

impl RegistryClient {
    pub async fn connect(connector: &Connector) -> Result<Self, Error> {
        Ok(Self {
            session: connector.connect_registry().await?,
        })
    }

    #[must_use]
    pub fn session(&self) -> &ClientSession {
        &self.session
    }

    pub async fn close(&self) {
        self.session.close().await;
    }

    pub async fn resolve(&self, namespace: &Name, slug: &Name) -> Result<AppRecord, Error> {
        let request = Message::Resolve(Resolve {
            namespace: namespace.clone(),
            slug: slug.clone(),
        });
        match self.session.request(&request).await? {
            Message::Resolved(resolved) => Ok(resolved.record),
            _ => Err(Error::UnexpectedReply("expected RESOLVED")),
        }
    }

    pub async fn list_apps(
        &self,
        query: &str,
        limit: u64,
        cursor: Option<Cursor>,
    ) -> Result<(AppsListed, AppsPage), Error> {
        let query = query
            .parse()
            .map_err(|_| Error::InvalidInput("query too long"))?;
        let request = Message::ListApps(ListApps {
            query,
            limit,
            cursor,
        });
        match self.session.request(&request).await? {
            Message::AppsListed(payload) => {
                let page = serde_json::from_slice(&payload.body)
                    .map_err(|_| Error::UnexpectedReply("APPS_LISTED body"))?;
                Ok((payload.data, page))
            }
            _ => Err(Error::UnexpectedReply("expected APPS_LISTED")),
        }
    }

    pub async fn host_register(
        &self,
        operation_id: Uuid7,
        endpoint: Endpoint,
        expected_revision: u64,
    ) -> Result<HostRegistered, Error> {
        let request = Message::HostRegister(HostRegister {
            operation_id,
            endpoint,
            expected_revision,
        });
        match self.session.request(&request).await? {
            Message::HostRegistered(done) => Ok(done),
            _ => Err(Error::UnexpectedReply("expected HOST_REGISTERED")),
        }
    }

    pub async fn heartbeat(&self, host_revision: u64) -> Result<HeartbeatAck, Error> {
        match self
            .session
            .request(&Message::HostHeartbeat(HostHeartbeat { host_revision }))
            .await?
        {
            Message::HeartbeatAck(ack) => Ok(ack),
            _ => Err(Error::UnexpectedReply("expected HEARTBEAT_ACK")),
        }
    }

    pub async fn register_app(&self, request: RegisterApp) -> Result<AppRecord, Error> {
        match self.session.request(&Message::RegisterApp(request)).await? {
            Message::AppRegistered(done) => Ok(done.record),
            _ => Err(Error::UnexpectedReply("expected APP_REGISTERED")),
        }
    }

    pub async fn update_app(&self, request: UpdateApp) -> Result<AppRecord, Error> {
        match self.session.request(&Message::UpdateApp(request)).await? {
            Message::AppUpdated(done) => Ok(done.record),
            _ => Err(Error::UnexpectedReply("expected APP_UPDATED")),
        }
    }

    pub async fn unregister_app(&self, request: UnregisterApp) -> Result<AppUnregistered, Error> {
        match self
            .session
            .request(&Message::UnregisterApp(request))
            .await?
        {
            Message::AppUnregistered(done) => Ok(done),
            _ => Err(Error::UnexpectedReply("expected APP_UNREGISTERED")),
        }
    }
}
