//! Calling an application's backend, with reconnects that never change an operation.

use std::sync::Arc;

use ilmek_core::session::{ClientError, ClientSession, ResponseStream};
use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::payload_hash;
use ilmek_protocol::wire::{
    AppCall, AppRecord, Cursor, Event, EventAck, Hash, Message, Mode, Payload, Subscribe,
    Subscribed, SubscriptionRef, Version,
};
use tokio::sync::Mutex;

use crate::connector::Connector;
use crate::error::Error;
use crate::retry::{RetryPolicy, retry_write};

/// A reply to an application call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallResult {
    pub operation_id: Option<Uuid7>,
    pub body: Vec<u8>,
}

/// Talks to one application's backend over a lazily opened, self-healing connection.
pub struct AppClient {
    connector: Arc<Connector>,
    record: AppRecord,
    client_version: Version,
    session: Mutex<Option<ClientSession>>,
    policy: RetryPolicy,
}

impl std::fmt::Debug for AppClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppClient")
            .field("app_id", &self.record.app_id)
            .finish_non_exhaustive()
    }
}

impl AppClient {
    /// `client_version` is the package version whose UI is making the calls.
    #[must_use]
    pub fn new(connector: Arc<Connector>, record: AppRecord, client_version: Version) -> Self {
        Self {
            connector,
            record,
            client_version,
            session: Mutex::new(None),
            policy: RetryPolicy::default(),
        }
    }

    #[must_use]
    pub fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    #[must_use]
    pub fn record(&self) -> &AppRecord {
        &self.record
    }

    async fn session(&self) -> Result<ClientSession, Error> {
        let mut guard = self.session.lock().await;
        if let Some(existing) = guard.as_ref().filter(|s| !s.is_closed()) {
            return Ok(existing.clone());
        }
        let address =
            std::net::SocketAddr::new(self.record.endpoint.ip.into(), self.record.endpoint.port);
        let opened = self
            .connector
            .connect_host(address, self.record.server_id, Mode::Control)
            .await?;
        *guard = Some(opened.clone());
        Ok(opened)
    }

    fn call_message(
        &self,
        method: &str,
        body: &[u8],
        operation_id: Option<Uuid7>,
    ) -> Result<Message, Error> {
        let method = method
            .parse()
            .map_err(|_| Error::InvalidInput("method name"))?;
        let hash = Hash::from_bytes(payload_hash(self.record.app_id, method_str(&method), body));
        Ok(Message::AppCall(Payload {
            data: AppCall {
                app_id: self.record.app_id,
                client_app_version: self.client_version,
                method,
                operation_id,
                payload_hash: hash,
            },
            body: body.to_vec(),
        }))
    }

    async fn attempt(&self, message: &Message) -> Result<CallResult, Error> {
        let session = self.session().await?;
        match session.request(message).await? {
            Message::AppResult(result) => Ok(CallResult {
                operation_id: result.data.operation_id,
                body: result.body,
            }),
            _ => Err(Error::UnexpectedReply("expected APP_RESULT")),
        }
    }

    /// A read: safe to repeat, so it is retried like any other idempotent request.
    pub async fn read(&self, method: &str, body: &[u8]) -> Result<CallResult, Error> {
        let message = self.call_message(method, body, None)?;
        retry_write(&self.policy, || self.attempt(&message)).await
    }

    /// A write. The operation id and body are fixed *once*, here; every retry — on the same
    /// or a new connection — repeats exactly those bytes, so the server applies it once.
    pub async fn write(
        &self,
        method: &str,
        body: &[u8],
        operation_id: Option<Uuid7>,
    ) -> Result<CallResult, Error> {
        let operation_id = operation_id.unwrap_or_else(Uuid7::generate);
        let message = self.call_message(method, body, Some(operation_id))?;
        retry_write(&self.policy, || self.attempt(&message)).await
    }

    /// Follows `topic`. Subscribe first, then fetch current state, then merge by event id.
    pub async fn subscribe(
        &self,
        topic: &str,
        cursor: Option<Cursor>,
    ) -> Result<EventSubscription, Error> {
        let request = Message::Subscribe(Subscribe {
            app_id: self.record.app_id,
            client_app_version: self.client_version,
            topic: topic
                .parse()
                .map_err(|_| Error::InvalidInput("topic name"))?,
            cursor,
        });
        let session = self.session().await?;
        let mut stream = session.open_stream(&request).await?;
        match stream.next().await? {
            Some(Message::Subscribed(subscribed)) => Ok(EventSubscription {
                session,
                stream,
                subscribed,
            }),
            _ => Err(Error::UnexpectedReply("expected SUBSCRIBED")),
        }
    }
}

fn method_str(method: &ilmek_protocol::wire::Method) -> &str {
    method.as_str()
}

/// A live subscription; call [`Self::acknowledge`] after storing each event's cursor.
pub struct EventSubscription {
    session: ClientSession,
    stream: ResponseStream,
    subscribed: Subscribed,
}

impl std::fmt::Debug for EventSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSubscription")
            .field("id", &self.subscribed.subscription_id)
            .finish_non_exhaustive()
    }
}

/// One delivered event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivered {
    pub event: Event,
    pub body: Vec<u8>,
}

impl EventSubscription {
    #[must_use]
    pub fn cursor(&self) -> &Cursor {
        &self.subscribed.cursor
    }

    #[must_use]
    pub fn subscription_id(&self) -> Uuid7 {
        self.subscribed.subscription_id
    }

    /// The next event; `Ok(None)` if the server ended the stream.
    pub async fn next(&mut self) -> Result<Option<Delivered>, Error> {
        match self.stream.next().await {
            Ok(Some(Message::Event(Payload { data, body }))) => {
                Ok(Some(Delivered { event: data, body }))
            }
            Ok(Some(_)) => Err(Error::UnexpectedReply("expected EVENT")),
            Ok(None) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn acknowledge(&self, delivered: &Delivered) -> Result<(), Error> {
        let ack = Message::EventAck(EventAck {
            subscription_id: delivered.event.subscription_id,
            sequence: delivered.event.sequence,
            cursor: delivered.event.cursor.clone(),
        });
        match self.session.request(&ack).await? {
            Message::EventAcked(_) => Ok(()),
            _ => Err(Error::UnexpectedReply("expected EVENT_ACKED")),
        }
    }

    pub async fn unsubscribe(&self) -> Result<(), Error> {
        let request = Message::Unsubscribe(SubscriptionRef {
            subscription_id: self.subscribed.subscription_id,
        });
        match self.session.request(&request).await {
            Ok(Message::Unsubscribed(_)) | Err(ClientError::Closed) => Ok(()),
            Ok(_) => Err(Error::UnexpectedReply("expected UNSUBSCRIBED")),
            Err(error) => Err(error.into()),
        }
    }
}
