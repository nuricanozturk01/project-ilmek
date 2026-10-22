#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::time::Duration;

use ilmek_app_sdk::{
    AppError, Backend, BackendConfig, CallContext, MethodKind, Runtime, SubscribeContext, Tuning,
    run_tuned,
};
use ilmek_core::transport::{FrameReader, FrameSender, Priority, spawn_writer};
use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::payload_hash;
use ilmek_protocol::ipc::{IpcCall, IpcMessage, IpcReady, IpcSubscribe};
use ilmek_protocol::wire::{
    AppResult, Cursor, ErrorCode, EventAck, Hash, Header, Payload, SubscriptionRef, Token, Version,
};
use ilmek_store::Migration;
use tempfile::TempDir;
use tokio::io::{ReadHalf, split};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "notes",
    sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL) STRICT;",
}];

struct Notes {
    runtime: Runtime,
}

impl Backend for Notes {
    fn methods(&self) -> &'static [(&'static str, MethodKind)] {
        &[
            ("notes.count", MethodKind::Read),
            ("notes.add", MethodKind::Write),
            ("notes.add_then_fail", MethodKind::Write),
        ]
    }

    async fn call(&self, context: CallContext, body: Vec<u8>) -> Result<Vec<u8>, AppError> {
        match context.method.as_str() {
            "notes.count" => {
                let count = self
                    .runtime
                    .db
                    .call(|c| {
                        Ok::<_, AppError>(
                            c.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get::<_, i64>(0))?,
                        )
                    })
                    .await?;
                Ok(format!("{{\"count\":{count}}}").into_bytes())
            }
            "notes.add" => {
                let (result, _) = self
                    .runtime
                    .operations
                    .write(&context, move |tx, events| {
                        tx.execute(
                            "INSERT INTO notes (body) VALUES (?1)",
                            [String::from_utf8_lossy(&body)],
                        )?;
                        events.append(tx, "notes.events", &body)?;
                        Ok(b"{\"ok\":true}".to_vec())
                    })
                    .await?;
                Ok(result)
            }
            _ => {
                let (result, _) = self
                    .runtime
                    .operations
                    .write(&context, |tx, _| {
                        tx.execute("INSERT INTO notes (body) VALUES ('half done')", [])?;
                        Err(AppError::new(ErrorCode::Internal, "disk on fire"))
                    })
                    .await?;
                Ok(result)
            }
        }
    }

    async fn authorize_subscribe(&self, context: &SubscribeContext) -> Result<(), AppError> {
        if context.topic.as_str() == "notes.events" {
            Ok(())
        } else {
            Err(AppError::forbidden("unknown topic"))
        }
    }
}

struct Host {
    dir: TempDir,
    sender: FrameSender,
    reader: FrameReader<ReadHalf<TcpStream>>,
    community: Uuid7,
    app: Uuid7,
    stash: std::collections::VecDeque<(Header, IpcMessage)>,
}

async fn start(tuning: Tuning) -> Host {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (community, app) = (Uuid7::generate(), Uuid7::generate());
    let token = Token::new(vec![7; 32]);
    let config = BackendConfig {
        token: token.clone(),
        ipc_port: listener.local_addr().unwrap().port(),
        community_id: community,
        app_id: app,
        data_dir: dir.path().join("data").display().to_string(),
    };
    tokio::spawn(run_tuned(config, MIGRATIONS, tuning, |runtime| Notes {
        runtime,
    }));

    let (stream, _) = listener.accept().await.unwrap();
    let (read_half, write_half) = split(stream);
    let (sender, _writer) = spawn_writer(write_half);
    let mut reader = FrameReader::new(read_half);
    let hello = reader.next().await.unwrap().unwrap();
    let (hdr, message) = IpcMessage::decode(&hello).unwrap();
    assert!(matches!(message, IpcMessage::Hello(h) if h.token == token && h.app_id == app));
    let ready = IpcMessage::Ready(IpcReady { app_id: app })
        .encode(hdr)
        .unwrap();
    sender.send(&ready, Priority::Normal).await.unwrap();
    Host {
        dir,
        sender,
        reader,
        community,
        app,
        stash: std::collections::VecDeque::new(),
    }
}

impl Host {
    async fn send(&self, request_id: Uuid7, message: &IpcMessage) {
        let header = Header {
            request_id,
            community_id: self.community,
        };
        self.sender
            .send(&message.encode(header).unwrap(), Priority::Normal)
            .await
            .unwrap();
    }

    async fn next(&mut self) -> (Header, IpcMessage) {
        if let Some(stashed) = self.stash.pop_front() {
            return stashed;
        }
        self.read().await
    }

    async fn read(&mut self) -> (Header, IpcMessage) {
        let frame = timeout(Duration::from_secs(5), self.reader.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        IpcMessage::decode(&frame).unwrap()
    }

    fn call(
        &self,
        method: &str,
        operation: Option<Uuid7>,
        principal: Uuid7,
        body: &[u8],
    ) -> IpcMessage {
        IpcMessage::Call(Payload {
            data: IpcCall {
                app_id: self.app,
                client_app_version: Version::new(1, 0, 0),
                method: method.parse().unwrap(),
                operation_id: operation,
                payload_hash: Hash::from_bytes(payload_hash(self.app, method, body)),
                principal_id: principal,
            },
            body: body.to_vec(),
        })
    }

    async fn invoke(
        &mut self,
        method: &str,
        operation: Option<Uuid7>,
        principal: Uuid7,
        body: &[u8],
    ) -> Result<Vec<u8>, ErrorCode> {
        let request = Uuid7::generate();
        let message = self.call(method, operation, principal, body);
        self.send(request, &message).await;
        let reply = loop {
            let (header, reply) = self.read().await;
            if header.request_id == request {
                break reply;
            }
            self.stash.push_back((header, reply));
        };
        match reply {
            IpcMessage::Result(Payload {
                body,
                data: AppResult { .. },
            }) => Ok(body),
            IpcMessage::Error(error) => Err(error.code),
            other => unreachable!("{other:?}"),
        }
    }

    async fn count(&mut self) -> i64 {
        let body = self
            .invoke("notes.count", None, Uuid7::generate(), b"{}")
            .await
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["count"]
            .as_i64()
            .unwrap()
    }
}

#[tokio::test]
async fn method_table_and_kinds_are_enforced_by_the_backend() {
    let mut host = start(Tuning::default()).await;
    let who = Uuid7::generate();
    assert_eq!(
        host.invoke("notes.nope", None, who, b"{}").await,
        Err(ErrorCode::NotFound)
    );
    assert_eq!(
        host.invoke("notes.add", None, who, b"{}").await,
        Err(ErrorCode::InvalidMessage)
    );
    assert_eq!(
        host.invoke("notes.count", Some(Uuid7::generate()), who, b"{}")
            .await,
        Err(ErrorCode::InvalidMessage)
    );
    assert_eq!(host.count().await, 0);
}

#[tokio::test]
async fn a_write_with_the_same_operation_id_is_applied_once() {
    let mut host = start(Tuning::default()).await;
    let (who, operation) = (Uuid7::generate(), Uuid7::generate());
    let first = host
        .invoke("notes.add", Some(operation), who, b"{\"t\":1}")
        .await
        .unwrap();
    let again = host
        .invoke("notes.add", Some(operation), who, b"{\"t\":1}")
        .await
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(host.count().await, 1);

    assert_eq!(
        host.invoke("notes.add", Some(operation), who, b"{\"t\":2}")
            .await,
        Err(ErrorCode::OperationConflict),
        "same id, different body"
    );
    let other_principal = Uuid7::generate();
    host.invoke("notes.add", Some(operation), other_principal, b"{\"t\":1}")
        .await
        .unwrap();
    assert_eq!(host.count().await, 2, "dedupe is scoped per principal");
}

#[tokio::test]
async fn parallel_duplicates_commit_exactly_once() {
    let mut host = start(Tuning::default()).await;
    let (who, operation) = (Uuid7::generate(), Uuid7::generate());
    let ids: Vec<Uuid7> = (0..8).map(|_| Uuid7::generate()).collect();
    for id in &ids {
        let message = host.call("notes.add", Some(operation), who, b"{\"t\":9}");
        host.send(*id, &message).await;
    }
    let mut bodies = Vec::new();
    for _ in &ids {
        let (_, reply) = host.next().await;
        let IpcMessage::Result(result) = reply else {
            unreachable!("{reply:?}")
        };
        bodies.push(result.body);
    }
    assert!(bodies.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(host.count().await, 1);
}

#[tokio::test]
async fn a_failed_write_rolls_back_and_leaves_no_dedupe_record() {
    let mut host = start(Tuning::default()).await;
    let (who, operation) = (Uuid7::generate(), Uuid7::generate());
    for _ in 0..2 {
        assert_eq!(
            host.invoke("notes.add_then_fail", Some(operation), who, b"{}")
                .await,
            Err(ErrorCode::Internal)
        );
    }
    assert_eq!(host.count().await, 0, "half-done change was rolled back");
    host.invoke("notes.add", Some(operation), who, b"{}")
        .await
        .unwrap();
    assert_eq!(
        host.count().await,
        1,
        "the id was never consumed by the failure"
    );
}

async fn subscribe(
    host: &mut Host,
    principal: Uuid7,
    topic: &str,
    cursor: Option<Cursor>,
) -> (Uuid7, Uuid7, Cursor) {
    let request = Uuid7::generate();
    let message = IpcMessage::Subscribe(IpcSubscribe {
        app_id: host.app,
        client_app_version: Version::new(1, 0, 0),
        topic: topic.parse().unwrap(),
        cursor,
        principal_id: principal,
    });
    host.send(request, &message).await;
    let (header, reply) = host.next().await;
    assert_eq!(header.request_id, request);
    let IpcMessage::Subscribed(subscribed) = reply else {
        unreachable!("{reply:?}")
    };
    (request, subscribed.subscription_id, subscribed.cursor)
}

async fn expect_event(host: &mut Host, request: Uuid7) -> (u64, Cursor, Vec<u8>) {
    let (header, reply) = host.next().await;
    assert_eq!(header.request_id, request);
    let IpcMessage::Event(event) = reply else {
        unreachable!("{reply:?}")
    };
    (event.data.sequence, event.data.cursor, event.body)
}

async fn add_note(host: &mut Host, who: Uuid7, n: u32) {
    host.invoke(
        "notes.add",
        Some(Uuid7::generate()),
        who,
        format!("{{\"n\":{n}}}").as_bytes(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn events_follow_commits_and_resume_from_a_cursor_without_loss_or_repeats() {
    let mut host = start(Tuning::default()).await;
    let who = Uuid7::generate();
    add_note(&mut host, who, 0).await;

    let (request, subscription, boundary) = subscribe(&mut host, who, "notes.events", None).await;
    add_note(&mut host, who, 1).await;
    let (_, reply) = (0, host.next().await.1);
    let IpcMessage::Event(first) = reply else {
        unreachable!("{reply:?}")
    };
    assert_eq!(first.data.sequence, 1);
    assert_eq!(
        first.body, b"{\"n\":1}",
        "history before subscribing is not replayed"
    );

    add_note(&mut host, who, 2).await;
    let (sequence, cursor, body) = {
        let (_, reply) = host.next().await;
        let IpcMessage::Event(second) = reply else {
            unreachable!("{reply:?}")
        };
        (second.data.sequence, second.data.cursor, second.body)
    };
    assert_eq!((sequence, body.as_slice()), (2, &b"{\"n\":2}"[..]));
    let ack = IpcMessage::EventAck(EventAck {
        subscription_id: subscription,
        sequence: 2,
        cursor: cursor.clone(),
    });
    let _ = (request, boundary);

    host.send(Uuid7::generate(), &ack).await;
    let acked_stop = Uuid7::generate();
    host.send(
        acked_stop,
        &IpcMessage::Unsubscribe(SubscriptionRef {
            subscription_id: subscription,
        }),
    )
    .await;
    loop {
        let (header, reply) = host.next().await;
        if header.request_id == acked_stop {
            assert!(matches!(reply, IpcMessage::Unsubscribed(_)));
            break;
        }
    }

    add_note(&mut host, who, 3).await;
    let (request, _, _) = subscribe(&mut host, who, "notes.events", Some(cursor)).await;
    let (sequence, _, body) = expect_event(&mut host, request).await;
    assert_eq!(
        (sequence, body.as_slice()),
        (1, &b"{\"n\":3}"[..]),
        "only events after the cursor"
    );
}

#[tokio::test]
async fn the_ack_window_limits_in_flight_events() {
    let mut host = start(Tuning::default()).await;
    let who = Uuid7::generate();
    let (request, subscription, _) = subscribe(&mut host, who, "notes.events", None).await;
    for n in 0..40 {
        add_note(&mut host, who, n).await;
    }

    let mut cursors = Vec::new();
    for expected in 1..=16u64 {
        let (sequence, cursor, _) = expect_event(&mut host, request).await;
        assert_eq!(sequence, expected);
        cursors.push(cursor);
    }
    assert!(
        timeout(Duration::from_millis(600), host.reader.next())
            .await
            .is_err(),
        "no event may be sent while 16 are unacknowledged"
    );

    let ack = |sequence: u64, cursor: &Cursor| {
        IpcMessage::EventAck(EventAck {
            subscription_id: subscription,
            sequence,
            cursor: cursor.clone(),
        })
    };
    host.send(Uuid7::generate(), &ack(4, &cursors[3])).await;
    let (header, reply) = host.next().await;
    assert!(
        matches!(reply, IpcMessage::EventAcked(a) if a.sequence == 4),
        "{header:?}"
    );
    for expected in 17..=20u64 {
        let (sequence, _, _) = expect_event(&mut host, request).await;
        assert_eq!(sequence, expected);
    }
}

#[tokio::test]
async fn out_of_order_or_forged_acks_are_refused() {
    let mut host = start(Tuning::default()).await;
    let who = Uuid7::generate();
    let (request, subscription, _) = subscribe(&mut host, who, "notes.events", None).await;
    for n in 0..3 {
        add_note(&mut host, who, n).await;
    }
    let mut cursors = Vec::new();
    for _ in 0..3 {
        cursors.push(expect_event(&mut host, request).await.1);
    }
    let attempt = |sequence: u64, cursor: Cursor| {
        IpcMessage::EventAck(EventAck {
            subscription_id: subscription,
            sequence,
            cursor,
        })
    };

    for (sequence, cursor) in [(9, cursors[2].clone()), (2, cursors[0].clone())] {
        let id = Uuid7::generate();
        host.send(id, &attempt(sequence, cursor)).await;
        let (header, reply) = host.next().await;
        assert_eq!(header.request_id, id);
        assert!(matches!(reply, IpcMessage::Error(e) if e.code == ErrorCode::InvalidMessage));
    }
    let id = Uuid7::generate();
    host.send(
        id,
        &IpcMessage::EventAck(EventAck {
            subscription_id: Uuid7::generate(),
            sequence: 1,
            cursor: cursors[0].clone(),
        }),
    )
    .await;
    assert!(matches!(host.next().await.1, IpcMessage::Error(e) if e.code == ErrorCode::NotFound));
}

#[tokio::test]
async fn cursors_are_bound_to_principal_and_topic_and_topics_are_authorized() {
    let mut host = start(Tuning::default()).await;
    let (alice, mallory) = (Uuid7::generate(), Uuid7::generate());
    let (_, _, cursor) = subscribe(&mut host, alice, "notes.events", None).await;

    let request = Uuid7::generate();
    let stolen = IpcMessage::Subscribe(IpcSubscribe {
        app_id: host.app,
        client_app_version: Version::new(1, 0, 0),
        topic: "notes.events".parse().unwrap(),
        cursor: Some(cursor),
        principal_id: mallory,
    });
    host.send(request, &stolen).await;
    assert!(
        matches!(host.next().await.1, IpcMessage::Error(e) if e.code == ErrorCode::InvalidMessage)
    );

    let request = Uuid7::generate();
    let other_topic = IpcMessage::Subscribe(IpcSubscribe {
        app_id: host.app,
        client_app_version: Version::new(1, 0, 0),
        topic: "admin.secrets".parse().unwrap(),
        cursor: None,
        principal_id: alice,
    });
    host.send(request, &other_topic).await;
    assert!(matches!(host.next().await.1, IpcMessage::Error(e) if e.code == ErrorCode::Forbidden));
}

#[tokio::test]
async fn a_consumer_that_stops_acknowledging_is_cut_off() {
    let mut host = start(Tuning {
        slow_consumer_after: Duration::from_millis(400),
    })
    .await;
    let who = Uuid7::generate();
    let (request, _, _) = subscribe(&mut host, who, "notes.events", None).await;
    add_note(&mut host, who, 1).await;
    let _ = expect_event(&mut host, request).await;

    let (header, reply) = host.next().await;
    assert_eq!(header.request_id, request);
    assert!(matches!(reply, IpcMessage::Error(e) if e.code == ErrorCode::SlowConsumer));
}

#[tokio::test]
async fn pruned_history_makes_old_cursors_expire() {
    let mut host = start(Tuning::default()).await;
    let who = Uuid7::generate();
    let (_, subscription, cursor) = subscribe(&mut host, who, "notes.events", None).await;
    host.send(
        Uuid7::generate(),
        &IpcMessage::Unsubscribe(SubscriptionRef {
            subscription_id: subscription,
        }),
    )
    .await;
    let _ = host.next().await;

    let db_path = host.dir.path().join("data/app.db");
    let direct = ilmek_store::Database::open(&db_path, MIGRATIONS).unwrap();
    let events = ilmek_app_sdk::Events::default();
    direct
        .call_blocking::<_, ilmek_store::StoreError>(|c| {
            let tx = c.transaction()?;
            for n in 0..10_050 {
                events
                    .append(&tx, "notes.events", format!("{{\"n\":{n}}}").as_bytes())
                    .unwrap();
            }
            tx.commit()?;
            Ok(())
        })
        .unwrap();

    let request = Uuid7::generate();
    host.send(
        request,
        &IpcMessage::Subscribe(IpcSubscribe {
            app_id: host.app,
            client_app_version: Version::new(1, 0, 0),
            topic: "notes.events".parse().unwrap(),
            cursor: Some(cursor),
            principal_id: who,
        }),
    )
    .await;
    assert!(
        matches!(host.next().await.1, IpcMessage::Error(e) if e.code == ErrorCode::CursorExpired)
    );
}
