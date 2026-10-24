#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ilmek_client::Source;
use ilmek_host::{AppState, BackendRow, Desired, publish};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::Version;
use ilmek_runtime::{AppSession, BridgeEvent, Client, EventSink};
use ilmek_testing::{
    BackendHandles, Community, PackageSpec, TestHost, build_package, notes_launcher, start_host,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Collector(Mutex<Vec<BridgeEvent>>);

impl EventSink for Collector {
    fn emit(&self, event: &BridgeEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

impl Collector {
    fn events(&self) -> Vec<BridgeEvent> {
        self.0.lock().unwrap().clone()
    }

    async fn wait_for(&self, what: &str, test: impl Fn(&[BridgeEvent]) -> bool) {
        for _ in 0..300 {
            if test(&self.events()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        unreachable!("timed out waiting for {what}: {:?}", self.events());
    }
}

struct Stack {
    community: Community,
    host: TestHost,
    backends: BackendHandles,
    client: Client,
    sink: Arc<Collector>,
    _scratch: tempfile::TempDir,
    _client_root: tempfile::TempDir,
}

fn files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "index.html",
            b"<html><script src=\"app.js\"></script></html>".to_vec(),
        ),
        ("app.js", b"console.log('app')".to_vec()),
        ("css/style.css", b"body{}".to_vec()),
    ]
}

async fn stack(permissions: Vec<Permission>, slug: &'static str) -> Stack {
    let community = Community::start().await;
    let owner = community.publisher("nuri").await;
    let (launcher, backends) = notes_launcher();
    let host = start_host(&community, Arc::new(launcher)).await;
    let scratch = tempfile::tempdir().unwrap();
    let spec = PackageSpec {
        namespace: "nuri",
        slug,
        app_id: Uuid7::generate(),
        publisher_id: owner.id,
        version: Version::new(1, 0, 0),
        backend_required: true,
        permissions,
        files: files(),
    };
    let package = build_package(scratch.path(), "app", &spec);
    let installed = host.installer.install(&package).await.unwrap();
    host.store
        .set_backend(
            installed.app_id,
            BackendRow {
                executable: "x".into(),
                args: vec![],
                accepted_versions: vec![],
            },
        )
        .await
        .unwrap();
    host.store
        .set_desired(installed.app_id, Desired::Running, false)
        .await
        .unwrap();
    loop {
        if host
            .store
            .status(installed.app_id)
            .await
            .unwrap()
            .is_some_and(|s| s.state == AppState::Running)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    publish(
        &owner.member.connector,
        &host.store,
        installed.app_id,
        host.peer_id,
    )
    .await
    .unwrap();

    let client_root = tempfile::tempdir().unwrap();
    Client::join(client_root.path(), &community.profile.to_json().unwrap()).unwrap();
    let sink = Arc::new(Collector::default());
    let client = Client::open(client_root.path()).unwrap();
    assert!(client.refresh().await.registry_reachable);
    Stack {
        community,
        host,
        backends,
        client,
        sink,
        _scratch: scratch,
        _client_root: client_root,
    }
}

async fn open(stack: &Stack, slug: &str) -> Arc<AppSession> {
    let (session, source) = stack
        .client
        .open_app(
            &format!("ilmek://nuri/{slug}"),
            stack.sink.clone(),
            |_| {},
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(source, Source::Registry);
    session
}

fn all_permissions() -> Vec<Permission> {
    vec![
        Permission::AppCall,
        Permission::AppSubscribe,
        Permission::StorageLocal,
    ]
}

async fn request(session: &Arc<AppSession>, body: Value) -> Value {
    serde_json::to_value(session.handle(body).await).unwrap()
}

fn call(method: &str, input: &Value, operation: Option<&str>) -> Value {
    json!({"v":1,"op":"call","method":method,"input":input,"write":operation.is_some(),"operationId":operation,"timeoutMs":null})
}

fn failure_code(response: &Value) -> &str {
    assert_eq!(response["ok"], false, "{response}");
    response["error"]["code"].as_str().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_can_read_write_and_repeat_a_write_safely_through_the_bridge() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;

    let operation = Uuid7::generate().to_string();
    let add = call("notes.add", &json!({"text":"merhaba"}), Some(&operation));
    let first = request(&session, add.clone()).await;
    let again = request(&session, add).await;
    assert_eq!(
        first, again,
        "a repeat with the same operationId returns the stored answer"
    );
    assert_eq!(first["ok"], true);

    let listed = request(&session, call("notes.list", &json!({}), None)).await;
    assert_eq!(listed["result"]["notes"].as_array().unwrap().len(), 1);
    assert_eq!(
        request(&session, json!({"v":1,"op":"connection.state"})).await["result"],
        "connected"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_bridge_calls_never_reach_the_server() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;
    let good_id = Uuid7::generate().to_string();

    let cases = [
        call("Bad Method", &json!({}), None),
        call("notes.list", &json!([1]), None),
        json!({"v":1,"op":"call","method":"notes.list","input":{},"write":true,"operationId":null,"timeoutMs":null}),
        json!({"v":1,"op":"call","method":"notes.list","input":{},"write":false,"operationId":good_id,"timeoutMs":null}),
        json!({"v":1,"op":"call","method":"notes.add","input":{},"write":true,"operationId":"not-a-uuid","timeoutMs":null}),
        json!({"v":1,"op":"call","method":"notes.list","input":{},"write":false,"operationId":null,"timeoutMs":5}),
        json!({"v":1,"op":"call","method":"notes.list","input":{},"write":false,"operationId":null,"timeoutMs":9_999_999}),
        json!({"v":1,"op":"call","method":"notes.list","input":{"x":"y".repeat(250_000)},"write":false,"operationId":null,"timeoutMs":null}),
        json!({"v":1,"op":"storage.get","key":"../escape"}),
        json!({"v":1,"op":"storage.get","key":""}),
    ];
    for case in cases {
        let response = request(&session, case.clone()).await;
        assert!(
            ["INVALID_MESSAGE", "TOO_LARGE"].contains(&failure_code(&response)),
            "{case} -> {response}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_only_gets_the_capabilities_its_manifest_declares() {
    let stack = stack(vec![Permission::AppCall], "notlar").await;
    let session = open(&stack, "notlar").await;

    let storage = request(
        &session,
        json!({"v":1,"op":"storage.set","key":"k","value":1}),
    )
    .await;
    assert_eq!(failure_code(&storage), "FORBIDDEN");
    let subscribe = request(
        &session,
        json!({"v":1,"op":"subscribe","topic":"notes.events","cursor":null}),
    )
    .await;
    assert_eq!(failure_code(&subscribe), "FORBIDDEN");
    assert_eq!(
        request(&session, call("notes.list", &json!({}), None)).await["ok"],
        true
    );

    let no_call = crate::stack(vec![Permission::StorageLocal], "baska").await;
    let session = open(&no_call, "baska").await;
    assert_eq!(
        failure_code(&request(&session, call("notes.list", &json!({}), None)).await),
        "FORBIDDEN"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn local_storage_is_private_bounded_and_survives_reopening() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;

    let set = json!({"v":1,"op":"storage.set","key":"draft:1","value":{"title":"taslak"}});
    assert_eq!(request(&session, set).await["ok"], true);
    let got = request(&session, json!({"v":1,"op":"storage.get","key":"draft:1"})).await;
    assert_eq!(got["result"], json!({"title":"taslak"}));
    assert_eq!(
        request(&session, json!({"v":1,"op":"storage.get","key":"missing"})).await["result"],
        Value::Null
    );

    let huge = json!({"v":1,"op":"storage.set","key":"big","value":"x".repeat(300_000)});
    assert_eq!(failure_code(&request(&session, huge).await), "TOO_LARGE");
    for n in 0..30 {
        let value = "y".repeat(200_000);
        let outcome = request(
            &session,
            json!({"v":1,"op":"storage.set","key":format!("fill{n}"),"value":value}),
        )
        .await;
        if outcome["ok"] == false {
            assert_eq!(failure_code(&outcome), "RESOURCE_EXHAUSTED");
            assert!(
                n >= 20,
                "the quota should allow several megabytes first, stopped at {n}"
            );
            break;
        }
        assert!(n < 29, "the quota was never enforced");
    }

    session.close();
    let reopened = open(&stack, "notlar").await;
    let got = request(&reopened, json!({"v":1,"op":"storage.get","key":"draft:1"})).await;
    assert_eq!(got["result"], json!({"title":"taslak"}));
    request(
        &reopened,
        json!({"v":1,"op":"storage.remove","key":"draft:1"}),
    )
    .await;
    assert_eq!(
        request(&reopened, json!({"v":1,"op":"storage.get","key":"draft:1"})).await["result"],
        Value::Null
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pages_receive_only_their_own_verified_files_under_a_strict_policy() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;

    let entry = session.serve("/");
    assert_eq!(
        (entry.status, entry.content_type),
        (200, "text/html; charset=utf-8")
    );
    assert_eq!(session.serve("/app.js?cachebust=1#frag").status, 200);
    assert_eq!(
        session.serve("/css/style.css").content_type,
        "text/css; charset=utf-8"
    );
    let headers: std::collections::HashMap<_, _> = entry.headers.iter().cloned().collect();
    assert!(headers["Content-Security-Policy"].contains("default-src 'none'"));
    assert_eq!(headers["X-Content-Type-Options"], "nosniff");

    for hostile in [
        "/manifest.json",
        "/manifest.sig",
        "/../manifest.json",
        "/css/../../manifest.json",
        "/%2e%2e/manifest.json",
        "/app.js/../app.js",
        "//etc/passwd",
        "/etc/passwd",
        "\\windows\\win.ini",
        "app.js",
        "/css",
        "/css/",
        "/APP.JS",
        "/app.js%00.png",
    ] {
        assert_eq!(session.serve(hostile).status, 404, "{hostile}");
    }

    let origin = session.origin();
    assert!(session.allows_navigation(&session.start_url()));
    assert!(session.allows_navigation(&format!("{origin}/css/style.css?x=1")));
    for refused in [
        "https://example.com/".to_owned(),
        "http://10.77.10.10:47401/".to_owned(),
        "file:///etc/passwd".to_owned(),
        "ilmekapp://app-01a1647f-de10-755f-beb4-79875d9e6368/index.html".to_owned(),
        format!("{origin}.evil.test/index.html"),
        format!("{origin}/not-in-package.html"),
        "javascript:alert(1)".to_owned(),
        "data:text/html,hi".to_owned(),
        "about:blank".to_owned(),
        "ilmek://nuri/forum".to_owned(),
    ] {
        assert!(!session.allows_navigation(&refused), "{refused}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_bootstrap_script_carries_the_verified_identity_and_the_sdk() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;
    let script = session.bootstrap_script();
    assert!(script.starts_with("window.__ILMEK_BOOTSTRAP__ = {"));
    assert!(script.contains(&session.record().app_id.to_string()));
    assert!(script.contains("ilmek_bridge"));
    assert!(session.host_name().starts_with("app-"));
}

#[tokio::test(flavor = "multi_thread")]
async fn events_flow_to_the_page_and_a_dropped_connection_resumes_without_duplicates() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;
    let subscribe = request(
        &session,
        json!({"v":1,"op":"subscribe","topic":"notes.events","cursor":null}),
    )
    .await;
    let subscription = subscribe["result"]["subscriptionId"]
        .as_str()
        .unwrap()
        .to_owned();

    let add = |n: u32| {
        call(
            "notes.add",
            &json!({"text":format!("not {n}")}),
            Some(&Uuid7::generate().to_string()),
        )
    };
    request(&session, add(1)).await;
    stack
        .sink
        .wait_for("the first event", |events| {
            events.iter().any(|e| matches!(e, BridgeEvent::Event { subscription_id, .. } if subscription_id.to_string() == subscription))
        })
        .await;

    stack.backends.crash_all();
    stack
        .sink
        .wait_for("the page to learn the connection dropped", |events| {
            events.iter().any(|e| {
                matches!(
                    e,
                    BridgeEvent::Connection {
                        state: "disconnected"
                    }
                )
            })
        })
        .await;
    for _ in 0..100 {
        if request(&session, call("notes.list", &json!({}), None)).await["ok"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    request(&session, add(2)).await;
    stack
        .sink
        .wait_for("the second event after reconnecting", |events| {
            events
                .iter()
                .filter(|e| matches!(e, BridgeEvent::Event { .. }))
                .count()
                >= 2
        })
        .await;
    tokio::time::sleep(Duration::from_millis(600)).await;

    let delivered: Vec<Value> = stack
        .sink
        .events()
        .into_iter()
        .filter_map(|e| match e {
            BridgeEvent::Event { event, .. } => Some(event),
            _ => None,
        })
        .collect();
    assert_eq!(
        delivered.len(),
        2,
        "no duplicates and no losses: {delivered:?}"
    );
    assert!(
        stack
            .sink
            .events()
            .iter()
            .any(|e| matches!(e, BridgeEvent::Connection { state: "connected" }))
    );
    let _ = (&stack.community, &stack.host);
}

#[tokio::test(flavor = "multi_thread")]
async fn unsubscribing_stops_delivery_and_closing_a_session_ends_everything() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;
    let subscribe = request(
        &session,
        json!({"v":1,"op":"subscribe","topic":"notes.events","cursor":null}),
    )
    .await;
    let id = subscribe["result"]["subscriptionId"]
        .as_str()
        .unwrap()
        .to_owned();

    request(
        &session,
        json!({"v":1,"op":"unsubscribe","subscriptionId":id}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = stack.sink.events().len();
    request(
        &session,
        call(
            "notes.add",
            &json!({"text":"x"}),
            Some(&Uuid7::generate().to_string()),
        ),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        stack
            .sink
            .events()
            .iter()
            .filter(|e| matches!(e, BridgeEvent::Event { .. }))
            .count(),
        0
    );
    let _ = before;

    let denied = request(
        &session,
        json!({"v":1,"op":"subscribe","topic":"admin.secrets","cursor":null}),
    )
    .await;
    assert_eq!(failure_code(&denied), "FORBIDDEN");
    session.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn cache_and_data_are_listed_and_removable() {
    let stack = stack(all_permissions(), "notlar").await;
    let session = open(&stack, "notlar").await;
    request(
        &session,
        json!({"v":1,"op":"storage.set","key":"k","value":1}),
    )
    .await;
    session.close();

    assert_eq!(stack.client.cached_apps().len(), 1);
    assert!(stack.client.storage_usage() > 0);
    assert!(
        stack
            .client
            .visited()
            .contains(&"ilmek://nuri/notlar".to_owned())
    );
    stack
        .client
        .set_bookmark("ilmek://nuri/notlar", true)
        .unwrap();
    assert_eq!(
        stack.client.bookmarks(),
        vec!["ilmek://nuri/notlar".to_owned()]
    );

    stack
        .client
        .remove_app("ilmek://nuri/notlar")
        .await
        .unwrap();
    assert!(stack.client.cached_apps().is_empty());
    let report = stack.client.diagnostics();
    assert!(
        !report.contains("BEGIN") && !report.contains("token"),
        "diagnostics must hold no secrets"
    );
}
