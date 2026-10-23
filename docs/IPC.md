# Backend IPC and the Rust SDK

PROTOCOL.md §16 is the normative contract between a host and an application backend. This file
explains how `ilmek-host` and the Rust SDK `ilmek-app-sdk` implement it, with the details an
application author needs. It adds no wire format: if this file and §16 differ, §16 wins.

The IPC is **local**, not a network service: loopback only, a random port, byte framing identical
to ILMEK/1 (PROTOCOL.md §5) but without TLS, with `IPC_*` message types that are rejected on the
network endpoint (test: `ipc_types_are_not_network_messages`,
`network_and_ipc_vocabularies_do_not_overlap`).

## Start-up

1. The host picks a loopback listener and starts the backend executable chosen with
   `ilmek app set-backend` (with its `--arg`s).
2. It writes one JSON object to the child's **stdin**, then closes it:

   ```json
   { "token": "<32+ random bytes, base64url>", "ipc_port": 41234,
     "community_id": "<UUIDv7>", "app_id": "<UUIDv7>", "data_dir": "/var/lib/ilmek/host/apps/<app_id>/data" }
   ```

   Secrets are never put on the command line or in logs. The token is valid for this process
   instance only.
3. `ilmek_app_sdk::run(MIGRATIONS, build)` reads that object (10 s deadline), creates `data_dir`,
   opens `<data_dir>/app.db` with SQLite in WAL mode and applies your forward-only migrations plus
   the SDK's own tables (`ilmek_operations`, `ilmek_events`), calls `build(Runtime)` to construct
   your backend, and connects to `127.0.0.1:ipc_port`.
4. First message from the backend: `IPC_HELLO { token, app_id }`. The host answers
   `IPC_READY { app_id }` (5 s deadline); a wrong token or application ID closes the socket.
   After `IPC_READY` the **host sends the requests** and the backend answers.

`Runtime { db, operations, events, data_dir, app_id, community_id }` is what your constructor
receives; `db` is the shared `ilmek_store::Database`.

## Calls

For each network `APP_CALL` the host checks the session, the application, the permission and the
hash of the exact body, then sends `IPC_CALL` — the `APP_CALL` fields plus `principal_id`,
**filled in by the host from the client's certificate**. Inside the backend process the SDK checks
the method table and turns the message into:

```rust
pub struct CallContext {
    pub app_id: Uuid7,
    pub method: Method,
    pub principal_id: Uuid7,        // authenticated by the host; never trust the body for identity
    pub client_app_version: Version,
    pub operation_id: Option<Uuid7>,
    pub payload_hash: Hash,         // host-verified hash of the exact request body
}

pub trait Backend: Send + Sync + 'static {
    fn methods(&self) -> &'static [(&'static str, MethodKind)];   // Read | Write; the complete table
    async fn call(&self, context: CallContext, body: Vec<u8>) -> Result<Vec<u8>, AppError>;
    async fn authorize_subscribe(&self, context: &SubscribeContext) -> Result<(), AppError>; // default: forbidden
}
```

`body` is the exact JSON object the client sent; return the exact JSON bytes of the reply. The SDK
answers `NOT_FOUND` for a method not in `methods()` and `INVALID_MESSAGE` for a read that carries
an operation ID or a write that lacks one (test: `method_table_and_kinds_are_enforced_by_the_backend`). `AppError` constructors (`invalid`, `not_found`, `forbidden`, …) map to the
protocol error codes. Each call runs in its own task: a panicking handler is reported as
`INTERNAL` and the connection survives.

## Writes: exactly once

```rust
let (reply, outcome) = self.runtime.operations.write(&context, move |tx, events| {
    tx.execute("INSERT INTO topics …", params![…])?;                 // business change
    events.append(tx, "forum.events", br#"{"kind":"topic"}"#)?;      // optional events
    Ok(reply_bytes)
}).await?;                                                           // outcome: Applied | Replayed
```

Inside one `BEGIN IMMEDIATE` transaction the SDK looks up `(principal_id, operation_id)`:
- found with the **same method and payload hash** → returns the stored reply without running your
  closure (`Replayed`);
- found with a different method or hash → `OPERATION_CONFLICT`;
- not found → runs your closure, stores the reply with the dedupe row, commits both together.

So a crash or a lost reply can never leave a business change without its dedupe record or the
reverse; the client retries with the same operation ID and gets the original answer. If your
closure returns an error nothing is stored and the operation ID stays unused. Concurrent duplicates
commit exactly once (tests: `parallel_duplicates_commit_exactly_once`,
`a_failed_write_rolls_back_and_leaves_no_dedupe_record`).

## Events and subscriptions

`events.append(tx, topic, body)` writes to the durable log inside your transaction; subscribers see
it only after commit. `IPC_SUBSCRIBE { …, principal_id }` first calls your `authorize_subscribe`.
Cursors are opaque, bound to principal and topic, and resume without loss or repeats. Rules from
PROTOCOL.md §10, implemented and tested:

- window of 16 unacknowledged events per subscription; the SDK stops sending until `IPC_EVENT_ACK`;
  an acknowledger that stalls for 30 s is cut off with `SLOW_CONSUMER`;
- out-of-order or forged acknowledgements are refused;
- the last 10 000 events per topic are kept; an older cursor answers `CURSOR_EXPIRED` (no silent
  loss);
- closing the network subscription ends the IPC subscription; request and subscription IDs from
  different clients are mapped to unique local ones so they cannot collide.

## Failure behaviour

- If the backend exits or the IPC socket closes, in-flight calls fail with `APP_UNAVAILABLE`; the
  host restarts the backend with exponential backoff (first delay `restart_backoff_ms`, doubling
  to 30 s) and **abandons it after repeated failures** until an administrator runs
  `ilmek app restart`. Other applications are unaffected.
- A write whose commit status is unknown to the client is recovered by repeating it with the same
  operation ID (see [APP_DEVELOPMENT](APP_DEVELOPMENT.md)); the host never guesses a rollback.
- The backend IPC cannot reach other applications, the registry, file downloads or arbitrary
  sockets; it is not a proxy.

## Trust

The backend executable is trusted code chosen by the host administrator; the token only stops
other local processes from impersonating it by accident. It is not a sandbox against a local
administrator or a malicious backend ([SECURITY](SECURITY.md#backends)).
