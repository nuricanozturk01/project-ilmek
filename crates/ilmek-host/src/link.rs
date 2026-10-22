//! The host's end of one backend's loopback IPC connection.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use ilmek_core::transport::{FrameReader, FrameSender, Priority, spawn_writer};
use ilmek_protocol::Uuid7;
use ilmek_protocol::ipc::{IpcCall, IpcMessage, IpcReady, IpcSubscribe};
use ilmek_protocol::wire::{EventAck, Header, Payload, SubscriptionRef, Token};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, split};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const HELLO_DEADLINE: Duration = Duration::from_secs(5);
const EVENT_BUFFER: usize = 32;

#[derive(Debug, Error)]
pub enum LinkError {
    #[error("the backend connection is closed")]
    Closed,
    #[error("backend broke the IPC protocol")]
    Protocol,
    #[error("backend did not identify itself correctly")]
    Handshake,
}

enum Route {
    Reply(oneshot::Sender<IpcMessage>),
    Stream(mpsc::Sender<IpcMessage>),
}

/// An authenticated connection to a running backend.
pub struct BackendLink {
    sender: FrameSender,
    community_id: Uuid7,
    routes: Mutex<HashMap<Uuid7, Route>>,
    closed: CancellationToken,
}

impl std::fmt::Debug for BackendLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendLink")
            .field("closed", &self.closed.is_cancelled())
            .finish_non_exhaustive()
    }
}

/// A started subscription: the backend's first answer plus the frames that follow.
#[derive(Debug)]
pub struct IpcSubscription {
    pub request_id: Uuid7,
    pub first: IpcMessage,
    pub events: mpsc::Receiver<IpcMessage>,
}

fn same_secret(a: &Token, b: &Token) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

impl BackendLink {
    /// Verifies `IPC_HELLO` (token and app id) on an accepted connection and answers `IPC_READY`.
    pub async fn accept<S>(
        stream: S,
        token: &Token,
        app_id: Uuid7,
        community_id: Uuid7,
    ) -> Result<std::sync::Arc<Self>, LinkError>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (read_half, write_half) = split(stream);
        let (sender, _writer) = spawn_writer(write_half);
        let mut reader = FrameReader::new(read_half);

        let frame = timeout(HELLO_DEADLINE, reader.next())
            .await
            .map_err(|_| LinkError::Handshake)?
            .map_err(|_| LinkError::Handshake)?
            .ok_or(LinkError::Handshake)?;
        let (header, message) = IpcMessage::decode(&frame).map_err(|_| LinkError::Handshake)?;
        let IpcMessage::Hello(hello) = message else {
            return Err(LinkError::Handshake);
        };
        if hello.app_id != app_id
            || !same_secret(&hello.token, token)
            || header.community_id != community_id
        {
            return Err(LinkError::Handshake);
        }
        let ready = IpcMessage::Ready(IpcReady { app_id })
            .encode(header)
            .map_err(|_| LinkError::Protocol)?;
        sender
            .send(&ready, Priority::Normal)
            .await
            .map_err(|_| LinkError::Closed)?;

        let link = std::sync::Arc::new(Self {
            sender,
            community_id,
            routes: Mutex::new(HashMap::new()),
            closed: CancellationToken::new(),
        });
        tokio::spawn(read_loop(
            reader,
            std::sync::Arc::downgrade(&link),
            link.closed.clone(),
        ));
        Ok(link)
    }

    #[must_use]
    pub fn closed(&self) -> &CancellationToken {
        &self.closed
    }

    /// Closes the connection; the backend sees EOF and exits.
    pub fn close(&self) {
        self.closed.cancel();
        self.routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub async fn call(&self, call: IpcCall, body: Vec<u8>) -> Result<IpcMessage, LinkError> {
        self.exchange(&IpcMessage::Call(Payload { data: call, body }))
            .await
    }

    pub async fn ack(&self, ack: EventAck) -> Result<IpcMessage, LinkError> {
        self.exchange(&IpcMessage::EventAck(ack)).await
    }

    pub async fn unsubscribe(&self, subscription_id: Uuid7) -> Result<IpcMessage, LinkError> {
        self.exchange(&IpcMessage::Unsubscribe(SubscriptionRef {
            subscription_id,
        }))
        .await
    }

    pub async fn subscribe(&self, subscribe: IpcSubscribe) -> Result<IpcSubscription, LinkError> {
        let request_id = Uuid7::generate();
        let (tx, mut rx) = mpsc::channel(EVENT_BUFFER);
        self.register(request_id, Route::Stream(tx))?;
        self.send(request_id, &IpcMessage::Subscribe(subscribe))
            .await?;
        let first = rx.recv().await.ok_or(LinkError::Closed)?;
        Ok(IpcSubscription {
            request_id,
            first,
            events: rx,
        })
    }

    async fn exchange(&self, message: &IpcMessage) -> Result<IpcMessage, LinkError> {
        let request_id = Uuid7::generate();
        let (tx, rx) = oneshot::channel();
        self.register(request_id, Route::Reply(tx))?;
        if let Err(error) = self.send(request_id, message).await {
            self.routes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&request_id);
            return Err(error);
        }
        tokio::select! {
            reply = rx => reply.map_err(|_| LinkError::Closed),
            () = self.closed.cancelled() => Err(LinkError::Closed),
        }
    }

    fn register(&self, request_id: Uuid7, route: Route) -> Result<(), LinkError> {
        if self.closed.is_cancelled() {
            return Err(LinkError::Closed);
        }
        self.routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request_id, route);
        Ok(())
    }

    async fn send(&self, request_id: Uuid7, message: &IpcMessage) -> Result<(), LinkError> {
        let frame = message
            .encode(Header {
                request_id,
                community_id: self.community_id,
            })
            .map_err(|_| LinkError::Protocol)?;
        self.sender
            .send(&frame, Priority::Normal)
            .await
            .map_err(|_| LinkError::Closed)
    }
}

async fn read_loop(
    mut reader: FrameReader<ReadHalf<impl AsyncRead + AsyncWrite + Send + 'static>>,
    link: std::sync::Weak<BackendLink>,
    closed: CancellationToken,
) {
    loop {
        let frame = tokio::select! {
            () = closed.cancelled() => break,
            frame = reader.next() => frame,
        };
        let Some(link) = link.upgrade() else { break };
        let Ok(Some(frame)) = frame else {
            link.close();
            return;
        };
        let Ok((header, message)) = IpcMessage::decode(&frame) else {
            link.close();
            return;
        };
        if !link.deliver(header.request_id, message).await {
            link.close();
            return;
        }
    }
}

impl BackendLink {
    /// Routes a backend frame; `false` means the backend referenced an unknown request.
    async fn deliver(&self, request_id: Uuid7, message: IpcMessage) -> bool {
        enum Target {
            Reply(oneshot::Sender<IpcMessage>),
            Stream(mpsc::Sender<IpcMessage>),
        }
        let terminal = matches!(message, IpcMessage::Error(_));
        let target = {
            let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
            match routes.get(&request_id) {
                None => return false,
                Some(Route::Reply(_)) => match routes.remove(&request_id) {
                    Some(Route::Reply(tx)) => Target::Reply(tx),
                    _ => return false,
                },
                Some(Route::Stream(tx)) => {
                    let tx = tx.clone();
                    if terminal {
                        routes.remove(&request_id);
                    }
                    Target::Stream(tx)
                }
            }
        };
        match target {
            Target::Reply(tx) => {
                let _ = tx.send(message);
            }
            Target::Stream(tx) => {
                if tx.send(message).await.is_err() {
                    self.routes
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&request_id);
                }
            }
        }
        true
    }
}
