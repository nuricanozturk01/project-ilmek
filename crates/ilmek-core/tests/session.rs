#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::sync::Arc;
use std::time::Duration;

use ilmek_core::clock::SystemClock;
use ilmek_core::session::{
    ClientError, ClientParams, ClientSession, ConnectionParams, Handler, Reply,
    ServerSessionConfig, SessionEnd, serve_connection,
};
use ilmek_core::transport::{FrameReader, Priority, spawn_writer};
use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::payload_hash;
use ilmek_protocol::wire::{
    AppCall, AppResult, AssetChunk, AssetEnd, AssetInfo, Cancel, CancelResult, ErrorCode, GetAsset,
    Hash, Header, Message, Mode, Payload, ServerRole, Subscribe, Subscribed, Version,
};
use tokio::io::duplex;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

const CHUNKS: u64 = 200;
const CHUNK_LEN: usize = 1024;

struct TestHandler;

impl Handler for TestHandler {
    async fn handle(&self, request: Message, reply: Reply) {
        match request {
            Message::AppCall(call) => handle_call(call, reply).await,
            Message::GetAsset(get) => stream_asset(&get, &reply).await,
            Message::Subscribe(sub) => {
                let ack = Message::Subscribed(Subscribed {
                    subscription_id: Uuid7::generate(),
                    app_id: sub.app_id,
                    topic: sub.topic,
                    cursor: "c0".parse().unwrap(),
                    window: 16,
                });
                reply.send(&ack).await.unwrap();
            }
            _ => {
                let _ = reply.fail(ErrorCode::NotFound, "unhandled", false).await;
            }
        }
    }
}

async fn handle_call(call: Payload<AppCall>, reply: Reply) {
    match call.data.method.as_str() {
        "echo" => {
            let result = AppResult {
                app_id: call.data.app_id,
                operation_id: call.data.operation_id,
            };
            let _ = reply
                .finish(&Message::AppResult(Payload {
                    data: result,
                    body: call.body,
                }))
                .await;
        }
        "slow" | "fast" => {
            if call.data.method.as_str() == "slow" {
                sleep(Duration::from_millis(300)).await;
            }
            let result = AppResult {
                app_id: call.data.app_id,
                operation_id: None,
            };
            let body = format!("{{\"m\":\"{}\"}}", call.data.method).into_bytes();
            let _ = reply
                .finish(&Message::AppResult(Payload { data: result, body }))
                .await;
        }
        "boom" => std::panic::resume_unwind(Box::new("handler exploded")),
        "silent" => {}
        _ => {
            let _ = reply
                .fail(ErrorCode::NotFound, "no such method", false)
                .await;
        }
    }
}

async fn stream_asset(get: &GetAsset, reply: &Reply) {
    let info = AssetInfo {
        app_id: get.app_id,
        version: get.version,
        path: get.path.clone(),
        asset_hash: get.asset_hash,
        total_size: CHUNKS * CHUNK_LEN as u64,
        offset: 0,
        chunk_size: 65_536,
    };
    if reply.send(&Message::AssetInfo(info)).await.is_err() {
        return;
    }
    for sequence in 0..CHUNKS {
        if reply.cancelled().is_cancelled() {
            return;
        }
        let chunk = Payload {
            data: AssetChunk {
                sequence,
                offset: sequence * CHUNK_LEN as u64,
            },
            body: vec![1; CHUNK_LEN],
        };
        if reply.send(&Message::AssetChunk(chunk)).await.is_err() {
            return;
        }
        sleep(Duration::from_millis(2)).await;
    }
    let end = AssetEnd {
        asset_hash: get.asset_hash,
        total_sent: CHUNKS * CHUNK_LEN as u64,
    };
    let _ = reply.finish(&Message::AssetEnd(end)).await;
}

struct Fixture {
    client: ClientSession,
    server: JoinHandle<SessionEnd>,
    app: Uuid7,
    shutdown: CancellationToken,
}

fn config(community: Uuid7, server_id: Uuid7) -> ServerSessionConfig {
    ServerSessionConfig::new(
        server_id,
        ServerRole::Host,
        community,
        Arc::new(SystemClock),
    )
}

async fn connect(mode: Mode, tune: impl FnOnce(&mut ServerSessionConfig)) -> Fixture {
    let community = Uuid7::generate();
    let server_id = Uuid7::generate();
    let mut cfg = config(community, server_id);
    tune(&mut cfg);
    let (near_end, far_end) = duplex(256 * 1024);
    let shutdown = CancellationToken::new();
    let params = ConnectionParams {
        remote_ip: None,
        config: Arc::new(cfg),
        handler: Arc::new(TestHandler),
        shutdown: shutdown.clone(),
    };
    let server = tokio::spawn(serve_connection(far_end, params));
    let client = ClientSession::open(
        near_end,
        ClientParams {
            mode,
            community_id: community,
            client_id: Uuid7::generate(),
            expected_role: ServerRole::Host,
            client_name: "test".into(),
            client_version: Version::new(1, 0, 0),
            request_timeout: Duration::from_secs(5),
        },
    )
    .await
    .unwrap();
    Fixture {
        client,
        server,
        app: Uuid7::generate(),
        shutdown,
    }
}

fn call(app: Uuid7, method: &str, body: &[u8]) -> Message {
    Message::AppCall(Payload {
        data: AppCall {
            app_id: app,
            client_app_version: Version::new(1, 0, 0),
            method: method.parse().unwrap(),
            operation_id: None,
            payload_hash: Hash::from_bytes(payload_hash(app, method, body)),
        },
        body: body.to_vec(),
    })
}

fn get_asset(app: Uuid7) -> Message {
    Message::GetAsset(GetAsset {
        app_id: app,
        version: Version::new(1, 0, 0),
        manifest_hash: Hash::from_bytes([1; 32]),
        path: "index.html".parse().unwrap(),
        asset_hash: Hash::from_bytes([2; 32]),
        offset: 0,
    })
}

fn remote_code(error: ClientError) -> ErrorCode {
    match error {
        ClientError::Remote(data) => data.code,
        other => unreachable!("expected a server error, got {other:?}"),
    }
}

fn result_body(message: Message) -> Vec<u8> {
    match message {
        Message::AppResult(payload) => payload.body,
        other => unreachable!("expected APP_RESULT, got {other:?}"),
    }
}

#[tokio::test]
async fn call_round_trip_returns_the_handler_body() {
    let fx = connect(Mode::Control, |_| {}).await;
    let reply = fx
        .client
        .request(&call(fx.app, "echo", b"{\"a\":1}"))
        .await
        .unwrap();
    assert_eq!(result_body(reply), b"{\"a\":1}");
}

#[tokio::test]
async fn app_call_before_hello_is_rejected_and_the_connection_ends() {
    let community = Uuid7::generate();
    let (client_io, server_io) = duplex(64 * 1024);
    let params = ConnectionParams {
        remote_ip: None,
        config: Arc::new(config(community, Uuid7::generate())),
        handler: Arc::new(TestHandler),
        shutdown: CancellationToken::new(),
    };
    let server = tokio::spawn(serve_connection(server_io, params));

    let (read_half, write_half) = tokio::io::split(client_io);
    let (sender, _task) = spawn_writer(write_half);
    let app = Uuid7::generate();
    let request_id = Uuid7::generate();
    let frame = call(app, "echo", b"{}")
        .encode(Header {
            request_id,
            community_id: community,
        })
        .unwrap();
    sender.send(&frame, Priority::Normal).await.unwrap();

    let mut reader = FrameReader::new(read_half);
    let reply = reader.next().await.unwrap().unwrap();
    let (header, message) = Message::decode(&reply).unwrap();
    let Message::Error(error) = message else {
        panic!("expected ERROR")
    };
    assert_eq!(error.code, ErrorCode::InvalidMessage);
    assert_eq!(header.request_id, request_id);
    assert_eq!(server.await.unwrap(), SessionEnd::ProtocolViolation);
}

#[tokio::test]
async fn message_types_are_confined_to_their_connection_mode() {
    let control = connect(Mode::Control, |_| {}).await;
    let mut stream = control
        .client
        .open_stream(&get_asset(control.app))
        .await
        .unwrap();
    let error = stream.next().await.unwrap_err();
    assert_eq!(remote_code(error), ErrorCode::InvalidMessage);

    let bulk = connect(Mode::Bulk, |_| {}).await;
    let error = bulk
        .client
        .request(&call(bulk.app, "echo", b"{}"))
        .await
        .unwrap_err();
    assert_eq!(remote_code(error), ErrorCode::InvalidMessage);
}

#[tokio::test]
async fn replies_are_matched_by_request_id_when_they_arrive_out_of_order() {
    let fx = connect(Mode::Control, |_| {}).await;
    let (slow_call, fast_call) = (call(fx.app, "slow", b"{}"), call(fx.app, "fast", b"{}"));
    let (slow, fast) = tokio::join!(fx.client.request(&slow_call), async {
        sleep(Duration::from_millis(50)).await;
        fx.client.request(&fast_call).await
    });
    assert_eq!(result_body(slow.unwrap()), b"{\"m\":\"slow\"}");
    assert_eq!(result_body(fast.unwrap()), b"{\"m\":\"fast\"}");
}

#[tokio::test]
async fn requests_beyond_the_inflight_limit_get_resource_exhausted() {
    let fx = connect(Mode::Control, |cfg| {
        cfg.max_control_inflight = 1;
    })
    .await;
    let (slow_call, fast_call) = (call(fx.app, "slow", b"{}"), call(fx.app, "fast", b"{}"));
    let (first, second) = tokio::join!(fx.client.request(&slow_call), async {
        sleep(Duration::from_millis(50)).await;
        fx.client.request(&fast_call).await
    });
    assert!(first.is_ok());
    assert_eq!(
        remote_code(second.unwrap_err()),
        ErrorCode::ResourceExhausted
    );
}

#[tokio::test]
async fn a_panicking_or_silent_handler_yields_internal_and_the_session_survives() {
    let fx = connect(Mode::Control, |_| {}).await;
    for method in ["boom", "silent"] {
        let error = fx
            .client
            .request(&call(fx.app, method, b"{}"))
            .await
            .unwrap_err();
        assert_eq!(remote_code(error), ErrorCode::Internal, "{method}");
    }
    assert!(
        fx.client
            .request(&call(fx.app, "echo", b"{}"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn cancel_ends_a_transfer_with_exactly_one_terminal_reply() {
    let fx = connect(Mode::Bulk, |_| {}).await;
    let mut stream = fx.client.open_stream(&get_asset(fx.app)).await.unwrap();
    let target = stream.request_id();

    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Message::AssetInfo(_))
    ));
    let mut chunks = 0;
    while chunks < 5 {
        assert!(matches!(
            stream.next().await.unwrap(),
            Some(Message::AssetChunk(_))
        ));
        chunks += 1;
    }

    let ack = fx
        .client
        .request(&Message::Cancel(Cancel {
            target_request_id: target,
        }))
        .await
        .unwrap();
    let Message::CancelAck(ack) = ack else {
        panic!("expected CANCEL_ACK")
    };
    assert_eq!(ack.result, CancelResult::Cancelled);

    let terminal = loop {
        match stream.next().await {
            Ok(Some(Message::AssetChunk(_))) => {}
            Ok(other) => panic!("unexpected frame {other:?}"),
            Err(error) => break error,
        }
    };
    assert_eq!(remote_code(terminal), ErrorCode::Cancelled);
    assert!(
        stream.next().await.unwrap().is_none(),
        "nothing may follow the terminal reply"
    );
    assert!(
        !fx.client.is_closed(),
        "late frames would have been a protocol violation"
    );
}

#[tokio::test]
async fn cancel_after_completion_reports_already_finished_and_unknown_ids_not_found() {
    let fx = connect(Mode::Bulk, |_| {}).await;
    let mut stream = fx.client.open_stream(&get_asset(fx.app)).await.unwrap();
    let target = stream.request_id();
    while stream.next().await.unwrap().is_some() {}

    let cancel = |id| {
        Message::Cancel(Cancel {
            target_request_id: id,
        })
    };
    let Message::CancelAck(done) = fx.client.request(&cancel(target)).await.unwrap() else {
        panic!("expected CANCEL_ACK")
    };
    assert_eq!(done.result, CancelResult::AlreadyFinished);

    let Message::CancelAck(unknown) = fx.client.request(&cancel(Uuid7::generate())).await.unwrap()
    else {
        panic!("expected CANCEL_ACK")
    };
    assert_eq!(unknown.result, CancelResult::NotFound);
}

#[tokio::test]
async fn a_second_transfer_on_one_bulk_connection_is_refused() {
    let fx = connect(Mode::Bulk, |_| {}).await;
    let mut first = fx.client.open_stream(&get_asset(fx.app)).await.unwrap();
    assert!(matches!(
        first.next().await.unwrap(),
        Some(Message::AssetInfo(_))
    ));

    let mut second = fx.client.open_stream(&get_asset(fx.app)).await.unwrap();
    assert_eq!(
        remote_code(second.next().await.unwrap_err()),
        ErrorCode::ResourceExhausted
    );
}

#[tokio::test]
async fn silent_peers_are_dropped_after_the_idle_timeout() {
    let fx = connect(Mode::Control, |cfg| {
        cfg.idle_timeout = Duration::from_millis(100);
    })
    .await;
    assert_eq!(
        timeout(Duration::from_secs(2), fx.server)
            .await
            .unwrap()
            .unwrap(),
        SessionEnd::IdleTimeout
    );
}

#[tokio::test]
async fn goodbye_is_acknowledged_and_ends_the_session() {
    let fx = connect(Mode::Control, |_| {}).await;
    fx.client.close().await;
    assert_eq!(fx.server.await.unwrap(), SessionEnd::Goodbye);
}

#[tokio::test]
async fn server_shutdown_announces_drain_and_new_requests_are_refused_client_side() {
    let fx = connect(Mode::Control, |cfg| {
        cfg.drain_grace = Duration::from_millis(200);
    })
    .await;
    fx.shutdown.cancel();
    assert_eq!(fx.server.await.unwrap(), SessionEnd::Shutdown);
    let error = fx
        .client
        .request(&call(fx.app, "echo", b"{}"))
        .await
        .unwrap_err();
    assert!(
        matches!(error, ClientError::Draining | ClientError::Closed),
        "{error:?}"
    );
}

#[tokio::test]
async fn request_limit_triggers_drain_then_server_draining() {
    let fx = connect(Mode::Control, |cfg| {
        cfg.request_limit = 2;
    })
    .await;
    for _ in 0..2 {
        fx.client
            .request(&call(fx.app, "echo", b"{}"))
            .await
            .unwrap();
    }
    let error = fx
        .client
        .request(&call(fx.app, "echo", b"{}"))
        .await
        .unwrap_err();
    assert_eq!(remote_code(error), ErrorCode::ServerDraining);
    let later = fx
        .client
        .request(&call(fx.app, "echo", b"{}"))
        .await
        .unwrap_err();
    assert!(matches!(later, ClientError::Draining), "{later:?}");
}

#[tokio::test]
async fn subscribe_acknowledgement_arrives_on_the_request_stream() {
    let fx = connect(Mode::Control, |_| {}).await;
    let subscribe = Message::Subscribe(Subscribe {
        app_id: fx.app,
        client_app_version: Version::new(1, 0, 0),
        topic: "forum.events".parse().unwrap(),
        cursor: None,
    });
    let mut stream = fx.client.open_stream(&subscribe).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Message::Subscribed(_))
    ));
}
