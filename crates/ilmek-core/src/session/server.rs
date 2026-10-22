use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ilmek_protocol::wire::{
    CancelAck, CancelResult, Drain, DrainReason, Empty, ErrorCode, Header, Hello, HelloAck,
    Message, Mode, Nonce, ServerRole, WireError,
};
use ilmek_protocol::{Envelope, Frame, MessageType, Uuid7};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{interval, timeout};
use tokio_util::sync::CancellationToken;

use super::modes::{Admission, admit};
use super::{error_message, header, priority};
use crate::clock::Clock;
use crate::peer::PeerCredentials;
use crate::transport::{FrameReader, FrameSender, Priority, TransportError, spawn_writer};

const HELLO_DEADLINE: Duration = Duration::from_secs(5);
const WRITER_FLUSH_DEADLINE: Duration = Duration::from_secs(5);
const PING_NONCE_LEN: usize = 16;
const FINISHED_TRANSFER_MEMORY: usize = 1024;

#[derive(Clone)]
pub struct ServerSessionConfig {
    pub server_id: Uuid7,
    pub role: ServerRole,
    pub community_id: Uuid7,
    /// `true` for the enrollment listener: only `enroll` connections, no client certificate.
    pub enrollment_endpoint: bool,
    pub max_control_inflight: u64,
    pub max_subscriptions: u64,
    pub idle_timeout: Duration,
    pub gate_interval: Duration,
    pub drain_grace: Duration,
    pub request_limit: u64,
    pub clock: Arc<dyn Clock>,
}

impl ServerSessionConfig {
    #[must_use]
    pub fn new(
        server_id: Uuid7,
        role: ServerRole,
        community_id: Uuid7,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            server_id,
            role,
            community_id,
            enrollment_endpoint: false,
            max_control_inflight: 16,
            max_subscriptions: if role == ServerRole::Host { 8 } else { 0 },
            idle_timeout: Duration::from_secs(90),
            gate_interval: Duration::from_mins(1),
            drain_grace: Duration::from_secs(30),
            request_limit: 100_000,
            clock,
        }
    }
}

impl std::fmt::Debug for ServerSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSessionConfig")
            .field("server_id", &self.server_id)
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

/// Facts about an established connection, as seen by request handlers.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: Uuid7,
    pub server_id: Uuid7,
    pub community_id: Uuid7,
    pub mode: Mode,
    /// Identity from the verified TLS certificate; `None` only on enrollment connections.
    pub peer: Option<PeerCredentials>,
    pub remote_ip: Option<IpAddr>,
    pub max_subscriptions: u64,
    /// Cancelled when the connection ends, for work that outlives a single request.
    pub closed: CancellationToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny { code: ErrorCode, close: bool },
}

/// Consulted at HELLO, on every request and periodically, so revocation reaches live sessions.
pub trait PeerGate: Send + Sync + 'static {
    fn check(&self, peer: Option<&PeerCredentials>) -> Verdict;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    Goodbye,
    PeerClosed,
    Shutdown,
    IdleTimeout,
    HelloTimeout,
    Revoked,
    ProtocolViolation,
    Transport,
}

pub trait Handler: Send + Sync + 'static {
    /// Handles one request. A request must end with exactly one terminal reply (`finish` or
    /// `fail`), except `SUBSCRIBE`, which answers `SUBSCRIBED` and may stream events later.
    fn handle(&self, request: Message, reply: Reply) -> impl Future<Output = ()> + Send;
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ReplyError {
    #[error("a terminal reply was already sent for this request")]
    AlreadyFinished,
    #[error("connection closed")]
    ConnectionClosed,
}

struct RequestState {
    terminal_sent: AsyncMutex<bool>,
    cancel: CancellationToken,
}

/// Write side of one request. Cloneable so a handler can keep streaming from a spawned task.
#[derive(Clone)]
pub struct Reply {
    request_id: Uuid7,
    info: Arc<SessionInfo>,
    sender: FrameSender,
    state: Arc<RequestState>,
}

impl std::fmt::Debug for Reply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl Reply {
    #[must_use]
    pub fn request_id(&self) -> Uuid7 {
        self.request_id
    }

    #[must_use]
    pub fn session(&self) -> &SessionInfo {
        &self.info
    }

    /// Triggered when the peer cancels this transfer or the connection ends.
    #[must_use]
    pub fn cancelled(&self) -> &CancellationToken {
        &self.state.cancel
    }

    /// Sends a non-terminal frame (`ASSET_INFO`, `ASSET_CHUNK`, `SUBSCRIBED`, `EVENT`, ...).
    pub async fn send(&self, message: &Message) -> Result<(), ReplyError> {
        let terminal = self.state.terminal_sent.lock().await;
        if *terminal {
            return Err(ReplyError::AlreadyFinished);
        }
        self.emit(message).await
    }

    /// Sends the terminal frame. Only the first terminal reply per request goes out.
    pub async fn finish(&self, message: &Message) -> Result<(), ReplyError> {
        let mut terminal = self.state.terminal_sent.lock().await;
        if *terminal {
            return Err(ReplyError::AlreadyFinished);
        }
        *terminal = true;
        self.emit(message).await
    }

    pub async fn fail(
        &self,
        code: ErrorCode,
        text: &str,
        retryable: bool,
    ) -> Result<(), ReplyError> {
        self.finish(&error_message(code, text, retryable)).await
    }

    pub async fn is_finished(&self) -> bool {
        *self.state.terminal_sent.lock().await
    }

    async fn emit(&self, message: &Message) -> Result<(), ReplyError> {
        send_message(
            &self.sender,
            header(self.request_id, self.info.community_id),
            message,
            None,
        )
        .await
        .map_err(|_| ReplyError::ConnectionClosed)
    }
}

async fn send_message(
    sender: &FrameSender,
    header: Header,
    message: &Message,
    priority_override: Option<Priority>,
) -> Result<(), TransportError> {
    let frame = message.encode(header).map_err(|_| TransportError::Closed)?;
    sender
        .send(
            &frame,
            priority_override.unwrap_or_else(|| priority(message.message_type())),
        )
        .await
}

enum CancelTarget {
    Active(Arc<RequestState>),
    Finished,
    Unknown,
    NotCancellable,
}

struct Active {
    state: Arc<RequestState>,
    cancellable: bool,
}

#[derive(Default)]
struct Requests {
    active: HashMap<Uuid7, Active>,
    finished_transfers: VecDeque<Uuid7>,
    seen: HashSet<Uuid7>,
    count: u64,
}

struct Shared {
    config: Arc<ServerSessionConfig>,
    info: Arc<SessionInfo>,
    sender: FrameSender,
    requests: Mutex<Requests>,
    inflight: Arc<Semaphore>,
    max_inflight: u32,
    draining: AtomicBool,
}

impl Shared {
    fn requests(&self) -> std::sync::MutexGuard<'_, Requests> {
        self.requests.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records a request id; `false` means the id was already used on this connection.
    fn register(&self, request_id: Uuid7, kind: MessageType) -> Option<Arc<RequestState>> {
        let mut requests = self.requests();
        requests.count += 1;
        if !requests.seen.insert(request_id) {
            return None;
        }
        let state = Arc::new(RequestState {
            terminal_sent: AsyncMutex::new(false),
            cancel: CancellationToken::new(),
        });
        let cancellable = kind == MessageType::GetAsset;
        requests.active.insert(
            request_id,
            Active {
                state: Arc::clone(&state),
                cancellable,
            },
        );
        Some(state)
    }

    fn complete(&self, request_id: Uuid7) {
        let mut requests = self.requests();
        let finished_transfer = requests
            .active
            .remove(&request_id)
            .is_some_and(|active| active.cancellable);
        if finished_transfer {
            requests.finished_transfers.push_back(request_id);
            if requests.finished_transfers.len() > FINISHED_TRANSFER_MEMORY {
                requests.finished_transfers.pop_front();
            }
        }
    }

    fn cancel_target(&self, target: Uuid7) -> CancelTarget {
        let requests = self.requests();
        match requests.active.get(&target) {
            Some(active) if active.cancellable => CancelTarget::Active(Arc::clone(&active.state)),
            Some(_) => CancelTarget::NotCancellable,
            None if requests.finished_transfers.contains(&target) => CancelTarget::Finished,
            None => CancelTarget::Unknown,
        }
    }

    fn request_count(&self) -> u64 {
        self.requests().count
    }

    fn cancel_all(&self) {
        for active in self.requests().active.values() {
            active.state.cancel.cancel();
        }
    }
}

/// Everything a connection needs besides its stream.
pub struct ConnectionParams<H> {
    pub peer: Option<PeerCredentials>,
    pub remote_ip: Option<IpAddr>,
    pub config: Arc<ServerSessionConfig>,
    pub handler: Arc<H>,
    pub gate: Arc<dyn PeerGate>,
    pub shutdown: CancellationToken,
}

impl<H> std::fmt::Debug for ConnectionParams<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionParams")
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}

/// Runs one accepted connection to completion.
pub async fn serve_connection<S, H>(stream: S, params: ConnectionParams<H>) -> SessionEnd
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    H: Handler,
{
    let (read_half, write_half) = tokio::io::split(stream);
    let (sender, writer_task) = spawn_writer(write_half);
    let mut reader = FrameReader::new(read_half);
    let mut tasks = JoinSet::new();
    let closed = CancellationToken::new();

    let end = run(&mut reader, &sender, &params, &mut tasks, &closed).await;

    closed.cancel();
    tasks.abort_all();
    drop(sender);
    let _ = timeout(WRITER_FLUSH_DEADLINE, writer_task).await;
    end
}

async fn run<R, H>(
    reader: &mut FrameReader<R>,
    sender: &FrameSender,
    params: &ConnectionParams<H>,
    tasks: &mut JoinSet<()>,
    closed: &CancellationToken,
) -> SessionEnd
where
    R: AsyncRead + Unpin,
    H: Handler,
{
    let config = &params.config;
    let (shared, hello_request_id) = match negotiate(reader, sender, params, config, closed).await {
        Ok(established) => established,
        Err(end) => return end,
    };

    let mut gate_tick = interval(config.gate_interval);
    gate_tick.tick().await;

    loop {
        tokio::select! {
            () = params.shutdown.cancelled(), if !shared.draining.load(Ordering::Relaxed) => {
                notify_drain(&shared, hello_request_id, DrainReason::Shutdown).await;
                wait_for_inflight(&shared).await;
                shared.cancel_all();
                return SessionEnd::Shutdown;
            }
            _ = gate_tick.tick() => {
                if let Verdict::Deny { code, close: true } = params.gate.check(params.peer.as_ref()) {
                    let revoked = error_message(code, "access revoked", false);
                    let _ = send_message(
                        sender,
                        header(hello_request_id, config.community_id),
                        &revoked,
                        None,
                    ).await;
                    shared.cancel_all();
                    return SessionEnd::Revoked;
                }
            }
            next = timeout(config.idle_timeout, reader.next()) => {
                let frame = match next {
                    Err(_) => return SessionEnd::IdleTimeout,
                    Ok(Ok(Some(frame))) => frame,
                    Ok(Ok(None)) => { shared.cancel_all(); return SessionEnd::PeerClosed; }
                    Ok(Err(_)) => { shared.cancel_all(); return SessionEnd::Transport; }
                };
                let flow = handle_frame(
                    &shared, &params.handler, params.gate.as_ref(), hello_request_id, &frame, tasks,
                ).await;
                let end = match flow {
                    Flow::Continue => continue,
                    Flow::Goodbye => { wait_for_inflight(&shared).await; SessionEnd::Goodbye }
                    Flow::Violation => SessionEnd::ProtocolViolation,
                    Flow::Revoked => SessionEnd::Revoked,
                };
                shared.cancel_all();
                return end;
            }
        }
    }
}

struct Rejection {
    request_id: Option<Uuid7>,
    code: ErrorCode,
    text: &'static str,
}

impl Rejection {
    fn new(request_id: Uuid7, code: ErrorCode, text: &'static str) -> Self {
        Self {
            request_id: Some(request_id),
            code,
            text,
        }
    }
}

fn validate_hello(
    frame: &Frame,
    peer: Option<&PeerCredentials>,
    config: &ServerSessionConfig,
    gate: &dyn PeerGate,
) -> Result<(Header, Hello), Rejection> {
    let Ok((hdr, Message::Hello(hello))) = Message::decode(frame) else {
        let request_id = Envelope::parse(&frame.metadata).ok().map(|e| e.request_id);
        return Err(Rejection {
            request_id,
            code: ErrorCode::InvalidMessage,
            text: "HELLO expected first",
        });
    };
    let id = hdr.request_id;
    if hello.version != 1 {
        return Err(Rejection::new(
            id,
            ErrorCode::UnsupportedVersion,
            "version 1 only",
        ));
    }
    if hdr.community_id != config.community_id {
        return Err(Rejection::new(
            id,
            ErrorCode::InvalidMessage,
            "community mismatch",
        ));
    }
    let mode_fits_endpoint = if config.enrollment_endpoint {
        hello.mode == Mode::Enroll && peer.is_none()
    } else {
        hello.mode != Mode::Enroll && peer.is_some()
    };
    if !mode_fits_endpoint {
        return Err(Rejection::new(
            id,
            ErrorCode::InvalidMessage,
            "mode not served here",
        ));
    }
    if let Verdict::Deny { code, .. } = gate.check(peer) {
        return Err(Rejection::new(id, code, "access denied"));
    }
    Ok((hdr, hello))
}

async fn negotiate<R: AsyncRead + Unpin, H>(
    reader: &mut FrameReader<R>,
    sender: &FrameSender,
    params: &ConnectionParams<H>,
    config: &Arc<ServerSessionConfig>,
    closed: &CancellationToken,
) -> Result<(Arc<Shared>, Uuid7), SessionEnd> {
    let frame = match timeout(HELLO_DEADLINE, reader.next()).await {
        Err(_) => return Err(SessionEnd::HelloTimeout),
        Ok(Ok(Some(frame))) => frame,
        Ok(Ok(None)) => return Err(SessionEnd::PeerClosed),
        Ok(Err(_)) => return Err(SessionEnd::Transport),
    };
    let (hdr, hello) =
        match validate_hello(&frame, params.peer.as_ref(), config, params.gate.as_ref()) {
            Ok(accepted) => accepted,
            Err(rejection) => {
                if let Some(request_id) = rejection.request_id {
                    let error = error_message(rejection.code, rejection.text, false);
                    let _ = send_message(
                        sender,
                        header(request_id, config.community_id),
                        &error,
                        None,
                    )
                    .await;
                }
                return Err(SessionEnd::ProtocolViolation);
            }
        };

    let (max_inflight, max_subscriptions) = match hello.mode {
        Mode::Control => (
            config.max_control_inflight.clamp(1, 16),
            config.max_subscriptions,
        ),
        Mode::Bulk | Mode::Enroll => (1, 0),
    };
    let info = Arc::new(SessionInfo {
        session_id: Uuid7::generate(),
        server_id: config.server_id,
        community_id: config.community_id,
        mode: hello.mode,
        peer: params.peer.clone(),
        remote_ip: params.remote_ip,
        max_subscriptions,
        closed: closed.clone(),
    });
    let ack = Message::HelloAck(HelloAck {
        version: 1,
        session_id: info.session_id,
        server_id: config.server_id,
        server_role: config.role,
        mode: hello.mode,
        max_inflight_requests: max_inflight,
        max_subscriptions,
        server_time_ms: config.clock.now_ms(),
    });
    if send_message(
        sender,
        header(hdr.request_id, config.community_id),
        &ack,
        None,
    )
    .await
    .is_err()
    {
        return Err(SessionEnd::Transport);
    }

    let max_inflight = u32::try_from(max_inflight).unwrap_or(1);
    let shared = Arc::new(Shared {
        config: Arc::clone(config),
        info,
        sender: sender.clone(),
        requests: Mutex::new(Requests::default()),
        inflight: Arc::new(Semaphore::new(max_inflight as usize)),
        max_inflight,
        draining: AtomicBool::new(false),
    });
    shared.requests().seen.insert(hdr.request_id);
    Ok((shared, hdr.request_id))
}

enum Flow {
    Continue,
    Goodbye,
    Violation,
    Revoked,
}

impl Shared {
    async fn reject(&self, request_id: Uuid7, code: ErrorCode, text: &'static str) {
        let error = error_message(code, text, false);
        let _ = send_message(
            &self.sender,
            header(request_id, self.config.community_id),
            &error,
            None,
        )
        .await;
    }

    async fn answer(&self, request_id: Uuid7, message: &Message) {
        let _ = send_message(
            &self.sender,
            header(request_id, self.config.community_id),
            message,
            None,
        )
        .await;
    }
}

/// Request-level checks that need no handler; `Err` carries what to do instead.
async fn precheck(
    shared: &Arc<Shared>,
    gate: &dyn PeerGate,
    hello_request_id: Uuid7,
    hdr: Header,
    message: &Message,
) -> Result<Arc<RequestState>, Flow> {
    let id = hdr.request_id;
    let kind = message.message_type();
    if hdr.community_id != shared.config.community_id {
        shared
            .reject(id, ErrorCode::InvalidMessage, "community mismatch")
            .await;
        return Err(Flow::Continue);
    }
    if admit(shared.info.mode, shared.config.role, kind) != Admission::Allowed {
        shared
            .reject(
                id,
                ErrorCode::InvalidMessage,
                "message not valid on this connection",
            )
            .await;
        return Err(Flow::Continue);
    }
    let Some(state) = shared.register(id, kind) else {
        shared
            .reject(id, ErrorCode::InvalidMessage, "request id reused")
            .await;
        return Err(Flow::Violation);
    };
    if let Verdict::Deny { code, close } = gate.check(shared.info.peer.as_ref()) {
        shared.reject(id, code, "access denied").await;
        shared.complete(id);
        return Err(if close { Flow::Revoked } else { Flow::Continue });
    }
    if shared.request_count() > shared.config.request_limit {
        notify_drain(shared, hello_request_id, DrainReason::RequestLimit).await;
    }
    if shared.draining.load(Ordering::Relaxed) {
        shared
            .reject(id, ErrorCode::ServerDraining, "open a new connection")
            .await;
        shared.complete(id);
        return Err(Flow::Continue);
    }
    Ok(state)
}

async fn handle_frame<H: Handler>(
    shared: &Arc<Shared>,
    handler: &Arc<H>,
    gate: &dyn PeerGate,
    hello_request_id: Uuid7,
    frame: &Frame,
    tasks: &mut JoinSet<()>,
) -> Flow {
    let (hdr, message) = match Message::decode(frame) {
        Ok(decoded) => decoded,
        Err(error) => return reject_undecodable(shared, frame, &error).await,
    };
    let state = match precheck(shared, gate, hello_request_id, hdr, &message).await {
        Ok(state) => state,
        Err(flow) => return flow,
    };
    let id = hdr.request_id;
    let kind = message.message_type();

    match message {
        Message::Ping(nonce) => {
            if nonce.nonce.as_bytes().len() == PING_NONCE_LEN {
                shared
                    .answer(id, &Message::Pong(Nonce { nonce: nonce.nonce }))
                    .await;
            } else {
                shared
                    .reject(id, ErrorCode::InvalidMessage, "nonce must be 16 bytes")
                    .await;
            }
            shared.complete(id);
            Flow::Continue
        }
        Message::Goodbye(_) => {
            shared.answer(id, &Message::GoodbyeAck(Empty {})).await;
            shared.complete(id);
            Flow::Goodbye
        }
        Message::Cancel(cancel) => {
            answer_cancel(shared, id, cancel.target_request_id).await;
            shared.complete(id);
            Flow::Continue
        }
        message => {
            let Ok(permit) = Arc::clone(&shared.inflight).try_acquire_owned() else {
                let busy = error_message(ErrorCode::ResourceExhausted, "too many requests", true);
                shared.answer(id, &busy).await;
                shared.complete(id);
                return Flow::Continue;
            };
            let reply = Reply {
                request_id: id,
                info: Arc::clone(&shared.info),
                sender: shared.sender.clone(),
                state,
            };
            let handler = Arc::clone(handler);
            let shared = Arc::clone(shared);
            tasks.spawn(async move {
                run_handler(handler, message, kind, reply.clone()).await;
                shared.complete(reply.request_id);
                drop(permit);
            });
            Flow::Continue
        }
    }
}

/// Runs the handler in its own task so a panic becomes `INTERNAL` instead of a dead connection.
async fn run_handler<H: Handler>(
    handler: Arc<H>,
    message: Message,
    kind: MessageType,
    reply: Reply,
) {
    let cancel = reply.cancelled().clone();
    let inner_reply = reply.clone();
    let mut inner = tokio::spawn(async move { handler.handle(message, inner_reply).await });
    let outcome = tokio::select! {
        result = &mut inner => Some(result),
        () = cancel.cancelled() => None,
    };
    match outcome {
        Some(Ok(())) => {
            if kind != MessageType::Subscribe && !reply.is_finished().await {
                let _ = reply
                    .fail(ErrorCode::Internal, "request ended without a reply", false)
                    .await;
            }
        }
        Some(Err(_)) => {
            let _ = reply
                .fail(ErrorCode::Internal, "internal error", false)
                .await;
        }
        None => inner.abort(),
    }
}

async fn answer_cancel(shared: &Arc<Shared>, cancel_request_id: Uuid7, target: Uuid7) {
    let community_id = shared.config.community_id;
    let respond = |message: Message| {
        let sender = shared.sender.clone();
        async move {
            // Same queue as the transfer's own frames, so the ack follows the terminal error.
            let _ = send_message(
                &sender,
                header(cancel_request_id, community_id),
                &message,
                Some(Priority::Normal),
            )
            .await;
        }
    };
    let ack = |result| {
        Message::CancelAck(CancelAck {
            target_request_id: target,
            result,
        })
    };

    match shared.cancel_target(target) {
        CancelTarget::Unknown => respond(ack(CancelResult::NotFound)).await,
        CancelTarget::Finished => respond(ack(CancelResult::AlreadyFinished)).await,
        CancelTarget::NotCancellable => {
            respond(error_message(
                ErrorCode::CannotCancel,
                "not a file transfer",
                false,
            ))
            .await;
        }
        CancelTarget::Active(state) => {
            state.cancel.cancel();
            let mut terminal = state.terminal_sent.lock().await;
            if *terminal {
                drop(terminal);
                respond(ack(CancelResult::AlreadyFinished)).await;
                return;
            }
            *terminal = true;
            let _ = send_message(
                &shared.sender,
                header(target, community_id),
                &error_message(ErrorCode::Cancelled, "transfer cancelled", false),
                Some(Priority::Normal),
            )
            .await;
            drop(terminal);
            respond(ack(CancelResult::Cancelled)).await;
        }
    }
}

async fn reject_undecodable(shared: &Arc<Shared>, frame: &Frame, error: &WireError) -> Flow {
    let Ok(envelope_parts) = ilmek_protocol::strict_json::parse_object(&frame.metadata) else {
        return Flow::Violation;
    };
    let request_id = envelope_parts
        .get("request_id")
        .and_then(|value| value.as_str())
        .and_then(|text| text.parse::<Uuid7>().ok());
    let Some(request_id) = request_id else {
        return Flow::Violation;
    };

    let code = match error {
        WireError::Envelope(ilmek_protocol::EnvelopeError::UnsupportedType(_)) => {
            ErrorCode::UnsupportedMessage
        }
        _ => ErrorCode::InvalidMessage,
    };
    let _ = send_message(
        &shared.sender,
        header(request_id, shared.config.community_id),
        &error_message(code, "message rejected", false),
        None,
    )
    .await;
    Flow::Continue
}

async fn notify_drain(shared: &Arc<Shared>, hello_request_id: Uuid7, reason: DrainReason) {
    if shared.draining.swap(true, Ordering::Relaxed) {
        return;
    }
    let grace_ms = u64::try_from(shared.config.drain_grace.as_millis())
        .unwrap_or(30_000)
        .min(30_000);
    let drain = Message::Drain(Drain { reason, grace_ms });
    let _ = send_message(
        &shared.sender,
        header(hello_request_id, shared.config.community_id),
        &drain,
        None,
    )
    .await;
}

async fn wait_for_inflight(shared: &Arc<Shared>) {
    let _ = timeout(
        shared.config.drain_grace,
        shared.inflight.acquire_many(shared.max_inflight),
    )
    .await;
}
