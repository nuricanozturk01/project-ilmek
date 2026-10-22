use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ilmek_core::transport::{FrameReader, FrameSender, Priority, spawn_writer};
use ilmek_protocol::Uuid7;
use ilmek_protocol::ipc::{IpcCall, IpcHello, IpcMessage, IpcSubscribe};
use ilmek_protocol::wire::{
    AppResult, ErrorData, Event, EventAcked, Header, Payload, Subscribed, SubscriptionRef,
};
use ilmek_store::{Database, Migration};
use tokio::io::{AsyncReadExt, split};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout};
use tokio_util::sync::CancellationToken;

use crate::backend::{Backend, CallContext, MethodKind, SubscribeContext};
use crate::config::BackendConfig;
use crate::dedupe::{self, Operations};
use crate::error::{AppError, SdkError};
use crate::events::{self, EventCursor, Events};

const STARTUP_READ_DEADLINE: Duration = Duration::from_secs(10);
const READY_DEADLINE: Duration = Duration::from_secs(5);
const EVENT_WINDOW: usize = 16;
const SLOW_CONSUMER_AFTER: Duration = Duration::from_secs(30);
const IDLE_POLL: Duration = Duration::from_millis(250);

/// Handles given to the backend when it is built.
#[derive(Debug, Clone)]
pub struct Runtime {
    pub db: Database,
    pub operations: Operations,
    pub events: Events,
    pub data_dir: PathBuf,
    pub app_id: Uuid7,
    pub community_id: Uuid7,
}

/// Reads the start-up configuration from stdin and serves until the host disconnects.
pub async fn run<B, F>(migrations: &[Migration], build: F) -> Result<(), SdkError>
where
    B: Backend,
    F: FnOnce(Runtime) -> B,
{
    let mut raw = Vec::new();
    timeout(
        STARTUP_READ_DEADLINE,
        tokio::io::stdin().read_to_end(&mut raw),
    )
    .await
    .map_err(|_| SdkError::Config("timed out reading stdin".into()))??;
    let config: BackendConfig =
        serde_json::from_slice(&raw).map_err(|e| SdkError::Config(e.to_string()))?;
    run_with_config(config, migrations, build).await
}

/// Timing knobs; the defaults are the protocol's values.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// How long unacknowledged events may stall before `SLOW_CONSUMER`.
    pub slow_consumer_after: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            slow_consumer_after: SLOW_CONSUMER_AFTER,
        }
    }
}

pub async fn run_with_config<B, F>(
    config: BackendConfig,
    migrations: &[Migration],
    build: F,
) -> Result<(), SdkError>
where
    B: Backend,
    F: FnOnce(Runtime) -> B,
{
    run_tuned(config, migrations, Tuning::default(), build).await
}

pub async fn run_tuned<B, F>(
    config: BackendConfig,
    migrations: &[Migration],
    tuning: Tuning,
    build: F,
) -> Result<(), SdkError>
where
    B: Backend,
    F: FnOnce(Runtime) -> B,
{
    let data_dir = PathBuf::from(&config.data_dir);
    std::fs::create_dir_all(&data_dir)?;
    let db = Database::open(&data_dir.join("app.db"), migrations)?;
    db.call_blocking::<_, ilmek_store::StoreError>(|connection| {
        connection.execute_batch(dedupe::SCHEMA)?;
        connection.execute_batch(events::SCHEMA)?;
        Ok(())
    })?;
    let events = Events::default();
    let runtime = Runtime {
        operations: Operations::new(db.clone(), events.clone()),
        db,
        events,
        data_dir,
        app_id: config.app_id,
        community_id: config.community_id,
    };

    let stream = TcpStream::connect(("127.0.0.1", config.ipc_port)).await?;
    let (read_half, write_half) = split(stream);
    let (sender, _writer) = spawn_writer(write_half);
    let mut reader = FrameReader::new(read_half);
    let community_id = config.community_id;

    let hello = IpcMessage::Hello(IpcHello {
        token: config.token.clone(),
        app_id: config.app_id,
    });
    send(
        &sender,
        &hello,
        Header {
            request_id: Uuid7::generate(),
            community_id,
        },
    )
    .await;
    let ready = timeout(READY_DEADLINE, reader.next())
        .await
        .map_err(|_| SdkError::Handshake("host did not answer IPC_HELLO"))?
        .map_err(|_| SdkError::Handshake("transport error"))?
        .ok_or(SdkError::Handshake("host closed the connection"))?;
    match IpcMessage::decode(&ready) {
        Ok((_, IpcMessage::Ready(r))) if r.app_id == config.app_id => {}
        _ => return Err(SdkError::Handshake("expected IPC_READY")),
    }

    let backend = Arc::new(build(runtime.clone()));
    serve(reader, sender, backend, runtime, tuning).await;
    Ok(())
}

async fn send(sender: &FrameSender, message: &IpcMessage, header: Header) {
    if let Ok(frame) = message.encode(header) {
        let _ = sender.send(&frame, Priority::Normal).await;
    }
}

struct Subscription {
    ack: mpsc::Sender<AckRequest>,
    cancel: CancellationToken,
}

struct AckRequest {
    request_id: Uuid7,
    ack: ilmek_protocol::wire::EventAck,
}

type Subscriptions = Arc<Mutex<HashMap<Uuid7, Subscription>>>;

async fn serve<R, B>(
    mut reader: FrameReader<R>,
    sender: FrameSender,
    backend: Arc<B>,
    runtime: Runtime,
    tuning: Tuning,
) where
    R: tokio::io::AsyncRead + Unpin,
    B: Backend,
{
    let subscriptions: Subscriptions = Arc::default();
    let community_id = runtime.community_id;
    while let Ok(Some(frame)) = reader.next().await {
        let Ok((header, message)) = IpcMessage::decode(&frame) else {
            break;
        };
        match message {
            IpcMessage::Call(call) => {
                tokio::spawn(handle_call(
                    Arc::clone(&backend),
                    sender.clone(),
                    header,
                    call,
                ));
            }
            IpcMessage::Subscribe(subscribe) => {
                tokio::spawn(handle_subscribe(
                    Arc::clone(&backend),
                    runtime.clone(),
                    sender.clone(),
                    Arc::clone(&subscriptions),
                    header,
                    subscribe,
                    tuning,
                ));
            }
            IpcMessage::EventAck(ack) => {
                let target = subscriptions
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&ack.subscription_id)
                    .map(|s| s.ack.clone());
                if let Some(channel) = target {
                    let request = AckRequest {
                        request_id: header.request_id,
                        ack,
                    };
                    let _ = channel.send(request).await;
                } else {
                    let error = failure(&AppError::not_found("unknown subscription"));
                    send(&sender, &IpcMessage::Error(error), header).await;
                }
            }
            IpcMessage::Unsubscribe(SubscriptionRef { subscription_id }) => {
                if let Some(sub) = subscriptions
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&subscription_id)
                {
                    sub.cancel.cancel();
                }
                let done = IpcMessage::Unsubscribed(SubscriptionRef { subscription_id });
                send(
                    &sender,
                    &done,
                    Header {
                        request_id: header.request_id,
                        community_id,
                    },
                )
                .await;
            }
            _ => break,
        }
    }
    for sub in subscriptions
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
    {
        sub.cancel.cancel();
    }
}

fn failure(error: &AppError) -> ErrorData {
    ErrorData {
        code: error.code,
        message: error.message.parse().unwrap_or_default(),
        retryable: error.retryable,
        retry_after_ms: None,
        details: serde_json::Map::new(),
    }
}

async fn handle_call<B: Backend>(
    backend: Arc<B>,
    sender: FrameSender,
    header: Header,
    call: Payload<IpcCall>,
) {
    let IpcCall {
        app_id,
        client_app_version,
        method,
        operation_id,
        payload_hash,
        principal_id,
    } = call.data;
    let outcome = match backend
        .methods()
        .iter()
        .find(|(name, _)| *name == method.as_str())
    {
        None => Err(AppError::not_found("no such method")),
        Some((_, MethodKind::Read)) if operation_id.is_some() => {
            Err(AppError::invalid("reads must not carry an operation id"))
        }
        Some((_, MethodKind::Write)) if operation_id.is_none() => {
            Err(AppError::invalid("writes need an operation id"))
        }
        Some(_) => {
            let context = CallContext {
                app_id,
                method,
                principal_id,
                client_app_version,
                operation_id,
                payload_hash,
            };
            let inner = Arc::clone(&backend);
            match tokio::spawn(async move { inner.call(context, call.body).await }).await {
                Ok(result) => result,
                Err(_) => Err(AppError::new(
                    ilmek_protocol::wire::ErrorCode::Internal,
                    "backend method failed",
                )),
            }
        }
    };
    let reply = match outcome {
        Ok(body) => IpcMessage::Result(Payload {
            data: AppResult {
                app_id,
                operation_id,
            },
            body,
        }),
        Err(error) => IpcMessage::Error(failure(&error)),
    };
    send(&sender, &reply, header).await;
}

async fn handle_subscribe<B: Backend>(
    backend: Arc<B>,
    runtime: Runtime,
    sender: FrameSender,
    subscriptions: Subscriptions,
    header: Header,
    subscribe: IpcSubscribe,
    tuning: Tuning,
) {
    let context = SubscribeContext {
        app_id: subscribe.app_id,
        topic: subscribe.topic.clone(),
        principal_id: subscribe.principal_id,
        client_app_version: subscribe.client_app_version,
        cursor: subscribe.cursor.clone(),
    };
    let start = match start_position(&*backend, &runtime, &context).await {
        Ok(start) => start,
        Err(error) => {
            send(&sender, &IpcMessage::Error(failure(&error)), header).await;
            return;
        }
    };

    let subscription_id = Uuid7::generate();
    let (ack_tx, ack_rx) = mpsc::channel(EVENT_WINDOW * 2);
    let cancel = CancellationToken::new();
    subscriptions
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            subscription_id,
            Subscription {
                ack: ack_tx,
                cancel: cancel.clone(),
            },
        );

    let cursor = EventCursor {
        seq: start,
        principal: context.principal_id,
        topic: context.topic.to_string(),
    };
    let Ok(encoded) = cursor.encode() else { return };
    let subscribed = IpcMessage::Subscribed(Subscribed {
        subscription_id,
        app_id: context.app_id,
        topic: context.topic.clone(),
        cursor: encoded,
        window: EVENT_WINDOW as u64,
    });
    send(&sender, &subscribed, header).await;

    let stream = EventStream {
        runtime,
        sender,
        header,
        subscription_id,
        context,
        position: start,
        ack_rx,
        cancel,
        tuning,
    };
    stream.run().await;
    subscriptions
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&subscription_id);
}

async fn start_position<B: Backend>(
    backend: &B,
    runtime: &Runtime,
    context: &SubscribeContext,
) -> Result<i64, AppError> {
    backend.authorize_subscribe(context).await?;
    let topic = context.topic.to_string();
    match &context.cursor {
        None => runtime.db.call(|c| events::current_boundary(c)).await,
        Some(cursor) => {
            let decoded = EventCursor::decode(cursor)?;
            if decoded.principal != context.principal_id || decoded.topic != topic {
                return Err(AppError::invalid("cursor belongs to another subscription"));
            }
            let seq = decoded.seq;
            let expired = runtime
                .db
                .call(move |c| events::is_expired(c, &topic, seq))
                .await?;
            if expired {
                return Err(AppError::new(
                    ilmek_protocol::wire::ErrorCode::CursorExpired,
                    "events before this cursor were pruned",
                ));
            }
            Ok(seq)
        }
    }
}

/// Delivers one subscription's events with a sliding window of unacknowledged events.
struct EventStream {
    runtime: Runtime,
    sender: FrameSender,
    header: Header,
    subscription_id: Uuid7,
    context: SubscribeContext,
    position: i64,
    ack_rx: mpsc::Receiver<AckRequest>,
    cancel: CancellationToken,
    tuning: Tuning,
}

impl EventStream {
    async fn run(mut self) {
        let topic = self.context.topic.to_string();
        let mut sent: Vec<(u64, i64)> = Vec::new();
        let mut acked_through = 0u64;
        let mut next_sequence = 1u64;
        let mut last_progress = Instant::now();
        let mut delivered = self.position;

        loop {
            let in_flight = sent.len();
            if in_flight < EVENT_WINDOW {
                let after = delivered;
                let limit = i64::try_from(EVENT_WINDOW - in_flight).unwrap_or(1);
                let topic_for_read = topic.clone();
                let batch = self
                    .runtime
                    .db
                    .call(move |c| events::read_after(c, &topic_for_read, after, limit))
                    .await;
                let Ok(batch) = batch else { return };
                if in_flight == 0 && !batch.is_empty() {
                    last_progress = Instant::now();
                }
                for stored in batch {
                    if self.emit(&topic, next_sequence, &stored).await.is_err() {
                        return;
                    }
                    sent.push((next_sequence, stored.seq));
                    delivered = stored.seq;
                    next_sequence += 1;
                }
            }

            let notified = self.runtime.events.notify.notified();
            tokio::select! {
                () = self.cancel.cancelled() => return,
                request = self.ack_rx.recv() => {
                    let Some(request) = request else { return };
                    match apply_ack(&mut sent, &mut acked_through, &request, &self.context, &topic) {
                        Ok(()) => {
                            last_progress = Instant::now();
                            let done = IpcMessage::EventAcked(EventAcked {
                                subscription_id: self.subscription_id,
                                sequence: request.ack.sequence,
                            });
                            let header = Header { request_id: request.request_id, community_id: self.header.community_id };
                            send(&self.sender, &done, header).await;
                        }
                        Err(error) => {
                            let header = Header { request_id: request.request_id, community_id: self.header.community_id };
                            send(&self.sender, &IpcMessage::Error(failure(&error)), header).await;
                        }
                    }
                }
                () = notified => {}
                () = tokio::time::sleep(IDLE_POLL) => {}
            }

            if !sent.is_empty() && last_progress.elapsed() >= self.tuning.slow_consumer_after {
                let slow = AppError::new(
                    ilmek_protocol::wire::ErrorCode::SlowConsumer,
                    "acknowledgements stopped advancing",
                );
                send(
                    &self.sender,
                    &IpcMessage::Error(failure(&slow)),
                    self.header,
                )
                .await;
                return;
            }
        }
    }

    async fn emit(
        &self,
        topic: &str,
        sequence: u64,
        stored: &events::StoredEvent,
    ) -> Result<(), ()> {
        let cursor = EventCursor {
            seq: stored.seq,
            principal: self.context.principal_id,
            topic: topic.to_owned(),
        }
        .encode()
        .map_err(|_| ())?;
        let event = IpcMessage::Event(Payload {
            data: Event {
                subscription_id: self.subscription_id,
                app_id: self.context.app_id,
                topic: self.context.topic.clone(),
                sequence,
                event_id: stored.event_id,
                cursor,
            },
            body: stored.body.clone(),
        });
        let frame = event.encode(self.header).map_err(|_| ())?;
        self.sender
            .send(&frame, Priority::Normal)
            .await
            .map_err(|_| ())
    }
}

/// Accepts an acknowledgement only for the oldest unacknowledged events, in order.
fn apply_ack(
    sent: &mut Vec<(u64, i64)>,
    acked_through: &mut u64,
    request: &AckRequest,
    context: &SubscribeContext,
    topic: &str,
) -> Result<(), AppError> {
    let ack = &request.ack;
    let position = sent
        .iter()
        .position(|(sequence, _)| *sequence == ack.sequence);
    let Some(position) = position.filter(|_| ack.sequence > *acked_through) else {
        return Err(AppError::invalid(
            "acknowledgement is ahead of delivered events",
        ));
    };
    let expected = EventCursor {
        seq: sent[position].1,
        principal: context.principal_id,
        topic: topic.to_owned(),
    }
    .encode()?;
    if ack.cursor != expected {
        return Err(AppError::invalid(
            "acknowledgement cursor does not match the event",
        ));
    }
    sent.drain(..=position);
    *acked_through = ack.sequence;
    Ok(())
}
