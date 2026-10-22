use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ilmek_protocol::wire::{
    ErrorData, Goodbye, GoodbyeReason, Hello, HelloAck, Message, Mode, Nonce, ServerRole, Version,
};
use ilmek_protocol::{MessageType, Uuid7};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

use super::{header, priority};
use crate::transport::{FrameReader, FrameSender, TransportError, spawn_writer};

const HELLO_DEADLINE: Duration = Duration::from_secs(5);
const KEEPALIVE_IDLE: Duration = Duration::from_secs(20);
const PONG_DEADLINE: Duration = Duration::from_secs(10);
const STREAM_BUFFER: usize = 32;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("server error {:?}: {}", .0.code, .0.message)]
    Remote(ErrorData),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("no reply within the deadline; the outcome of the request is unknown")]
    Timeout,
    #[error("connection closed")]
    Closed,
    #[error("server is draining; open a new connection")]
    Draining,
    #[error("protocol violation: {0}")]
    Protocol(&'static str),
}

#[derive(Debug, Clone)]
pub struct ClientParams {
    pub mode: Mode,
    pub community_id: Uuid7,
    pub expected_server_id: Uuid7,
    pub expected_role: ServerRole,
    pub client_name: String,
    pub client_version: Version,
    pub request_timeout: Duration,
}

type Delivery = Result<Message, ClientError>;

enum Route {
    Single(oneshot::Sender<Delivery>),
    Stream(mpsc::Sender<Delivery>),
    /// Reply is still owed but nobody waits for it any more (timeout, dropped stream).
    Discard,
}

struct Inner {
    sender: FrameSender,
    community_id: Uuid7,
    request_timeout: Duration,
    routes: Mutex<HashMap<Uuid7, Route>>,
    draining: AtomicBool,
    started: Instant,
    last_sent_ms: AtomicU64,
    shutdown: CancellationToken,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

impl Inner {
    fn routes(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid7, Route>> {
        self.routes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn send(&self, request_id: Uuid7, message: &Message) -> Result<(), ClientError> {
        let frame = message
            .encode(header(request_id, self.community_id))
            .map_err(|_| ClientError::Protocol("message could not be encoded"))?;
        self.sender
            .send(&frame, priority(message.message_type()))
            .await?;
        let elapsed = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.last_sent_ms.store(elapsed, Ordering::Relaxed);
        Ok(())
    }

    fn fail_all(&self, error: &dyn Fn() -> ClientError) {
        let routes = std::mem::take(&mut *self.routes());
        for route in routes.into_values() {
            match route {
                Route::Single(tx) => {
                    let _ = tx.send(Err(error()));
                }
                Route::Stream(tx) => {
                    let _ = tx.try_send(Err(error()));
                }
                Route::Discard => {}
            }
        }
        self.shutdown.cancel();
    }

    fn register(&self, request_id: Uuid7, route: Route) -> Result<(), ClientError> {
        if self.shutdown.is_cancelled() {
            return Err(ClientError::Closed);
        }
        if self.draining.load(Ordering::Relaxed) {
            return Err(ClientError::Draining);
        }
        self.routes().insert(request_id, route);
        Ok(())
    }

    async fn exchange(
        &self,
        message: &Message,
        deadline: Duration,
    ) -> Result<Message, ClientError> {
        let request_id = Uuid7::generate();
        let (tx, rx) = oneshot::channel();
        self.register(request_id, Route::Single(tx))?;
        if let Err(error) = self.send(request_id, message).await {
            self.routes().remove(&request_id);
            return Err(error);
        }
        match timeout(deadline, rx).await {
            Ok(Ok(delivery)) => delivery,
            Ok(Err(_)) => Err(ClientError::Closed),
            Err(_) => {
                self.abandon(request_id);
                Err(ClientError::Timeout)
            }
        }
    }

    fn abandon(&self, request_id: Uuid7) {
        if let Some(route) = self.routes().get_mut(&request_id) {
            *route = Route::Discard;
        }
    }
}

/// One established ILMEK/1 connection from the client side.
#[derive(Clone)]
pub struct ClientSession {
    inner: Arc<Inner>,
    ack: Arc<HelloAck>,
}

impl std::fmt::Debug for ClientSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientSession")
            .field("session_id", &self.ack.session_id)
            .finish_non_exhaustive()
    }
}

impl ClientSession {
    /// Performs HELLO over an already authenticated stream.
    pub async fn open<S>(stream: S, params: ClientParams) -> Result<Self, ClientError>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (read_half, write_half) = tokio::io::split(stream);
        let (sender, _writer) = spawn_writer(write_half);
        let mut reader = FrameReader::new(read_half);

        let hello_id = Uuid7::generate();
        let hello = Message::Hello(Hello {
            version: 1,
            mode: params.mode,
            client_name: params
                .client_name
                .parse()
                .map_err(|_| ClientError::Protocol("client name"))?,
            client_version: params.client_version,
        });
        let frame = hello
            .encode(header(hello_id, params.community_id))
            .map_err(|_| ClientError::Protocol("hello encoding"))?;
        sender.send(&frame, priority(MessageType::Hello)).await?;

        let reply = timeout(HELLO_DEADLINE, reader.next())
            .await
            .map_err(|_| ClientError::Timeout)??
            .ok_or(ClientError::Closed)?;
        let (reply_header, reply) = Message::decode(&reply)
            .map_err(|_| ClientError::Protocol("undecodable HELLO reply"))?;
        let ack = match reply {
            Message::HelloAck(ack) if reply_header.request_id == hello_id => ack,
            Message::Error(error) => return Err(ClientError::Remote(error)),
            _ => return Err(ClientError::Protocol("expected HELLO_ACK")),
        };
        let acceptable = ack.version == 1
            && ack.mode == params.mode
            && ack.server_id == params.expected_server_id
            && ack.server_role == params.expected_role
            && reply_header.community_id == params.community_id;
        if !acceptable {
            return Err(ClientError::Protocol(
                "HELLO_ACK does not match the connection",
            ));
        }

        let inner = Arc::new(Inner {
            sender,
            community_id: params.community_id,
            request_timeout: params.request_timeout,
            routes: Mutex::new(HashMap::new()),
            draining: AtomicBool::new(false),
            started: Instant::now(),
            last_sent_ms: AtomicU64::new(0),
            shutdown: CancellationToken::new(),
        });
        tokio::spawn(read_loop(
            reader,
            Arc::downgrade(&inner),
            inner.shutdown.clone(),
        ));
        tokio::spawn(keepalive_loop(
            Arc::downgrade(&inner),
            inner.shutdown.clone(),
        ));
        Ok(Self {
            inner,
            ack: Arc::new(ack),
        })
    }

    #[must_use]
    pub fn hello_ack(&self) -> &HelloAck {
        &self.ack
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.inner.shutdown.is_cancelled()
    }

    /// Sends a request and waits for its single terminal reply.
    ///
    /// A [`ClientError::Timeout`] says nothing about whether the server acted on the request.
    pub async fn request(&self, message: &Message) -> Result<Message, ClientError> {
        self.request_within(message, self.inner.request_timeout)
            .await
    }

    pub async fn request_within(
        &self,
        message: &Message,
        deadline: Duration,
    ) -> Result<Message, ClientError> {
        self.inner.exchange(message, deadline).await
    }

    /// Starts a request answered by several frames (`GET_ASSET`, `SUBSCRIBE`).
    pub async fn open_stream(&self, message: &Message) -> Result<ResponseStream, ClientError> {
        let request_id = Uuid7::generate();
        let (tx, rx) = mpsc::channel(STREAM_BUFFER);
        self.inner.register(request_id, Route::Stream(tx))?;
        if let Err(error) = self.inner.send(request_id, message).await {
            self.inner.routes().remove(&request_id);
            return Err(error);
        }
        Ok(ResponseStream {
            request_id,
            rx,
            inner: Arc::clone(&self.inner),
        })
    }

    /// Orderly shutdown: `GOODBYE`, then close.
    pub async fn close(&self) {
        let goodbye = Message::Goodbye(Goodbye {
            reason: GoodbyeReason::User,
        });
        let _ = self.request_within(&goodbye, Duration::from_secs(5)).await;
        self.inner.shutdown.cancel();
    }
}

/// Frames answering one multi-frame request, in arrival order.
pub struct ResponseStream {
    request_id: Uuid7,
    rx: mpsc::Receiver<Delivery>,
    inner: Arc<Inner>,
}

impl std::fmt::Debug for ResponseStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseStream")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl ResponseStream {
    #[must_use]
    pub fn request_id(&self) -> Uuid7 {
        self.request_id
    }

    /// Next frame; `Ok(None)` after the terminal frame was delivered.
    pub async fn next(&mut self) -> Result<Option<Message>, ClientError> {
        self.rx.recv().await.transpose()
    }

    /// Like [`Self::next`] with a deadline per frame.
    pub async fn next_within(
        &mut self,
        deadline: Duration,
    ) -> Result<Option<Message>, ClientError> {
        timeout(deadline, self.next())
            .await
            .map_err(|_| ClientError::Timeout)?
    }
}

impl Drop for ResponseStream {
    fn drop(&mut self) {
        self.inner.abandon(self.request_id);
    }
}

async fn read_loop<R: AsyncRead + Unpin>(
    mut reader: FrameReader<R>,
    inner: std::sync::Weak<Inner>,
    shutdown: CancellationToken,
) {
    loop {
        let frame = tokio::select! {
            () = shutdown.cancelled() => return,
            next = reader.next() => next,
        };
        let Some(inner) = inner.upgrade() else { return };
        let frame = match frame {
            Ok(Some(frame)) => frame,
            Ok(None) => return inner.fail_all(&|| ClientError::Closed),
            Err(_) => return inner.fail_all(&|| ClientError::Protocol("transport failure")),
        };
        let Ok((hdr, message)) = Message::decode(&frame) else {
            return inner.fail_all(&|| ClientError::Protocol("undecodable frame"));
        };
        if hdr.community_id != inner.community_id {
            return inner.fail_all(&|| ClientError::Protocol("community mismatch"));
        }
        if let Message::Drain(_) = message {
            inner.draining.store(true, Ordering::Relaxed);
            continue;
        }
        if !route(&inner, hdr.request_id, message).await {
            return inner.fail_all(&|| ClientError::Protocol("reply to unknown request"));
        }
    }
}

/// Delivers a frame to its waiter. Returns `false` if the request id is unknown.
async fn route(inner: &Arc<Inner>, request_id: Uuid7, message: Message) -> bool {
    let terminal = matches!(
        message.message_type(),
        MessageType::Error | MessageType::AssetEnd
    );
    let delivery = match message {
        Message::Error(error) => Err(ClientError::Remote(error)),
        other => Ok(other),
    };

    let target = {
        let mut routes = inner.routes();
        match routes.get(&request_id) {
            None => return false,
            Some(Route::Single(_)) => routes.remove(&request_id),
            Some(Route::Stream(tx)) => {
                let tx = tx.clone();
                if terminal {
                    routes.remove(&request_id);
                }
                Some(Route::Stream(tx))
            }
            Some(Route::Discard) => {
                if terminal {
                    routes.remove(&request_id);
                }
                None
            }
        }
    };
    match target {
        Some(Route::Single(tx)) => {
            let _ = tx.send(delivery);
        }
        Some(Route::Stream(tx)) if tx.send(delivery).await.is_err() => inner.abandon(request_id),
        _ => {}
    }
    true
}

async fn keepalive_loop(inner: std::sync::Weak<Inner>, shutdown: CancellationToken) {
    loop {
        let Some(alive) = inner.upgrade() else { return };
        let last =
            alive.started + Duration::from_millis(alive.last_sent_ms.load(Ordering::Relaxed));
        let due = last + KEEPALIVE_IDLE;
        drop(alive);

        tokio::select! {
            () = shutdown.cancelled() => return,
            () = sleep_until(due) => {}
        }
        let Some(alive) = inner.upgrade() else { return };
        let idle = alive.started.elapsed()
            >= Duration::from_millis(alive.last_sent_ms.load(Ordering::Relaxed)) + KEEPALIVE_IDLE;
        if !idle {
            continue;
        }
        let ping = Message::Ping(Nonce {
            nonce: ilmek_protocol::wire::Token::new(Uuid7::generate().as_bytes().to_vec()),
        });
        let alive_result = alive.exchange(&ping, PONG_DEADLINE).await;
        if alive_result.is_err() {
            alive.fail_all(&|| ClientError::Protocol("keepalive failed"));
            return;
        }
    }
}
