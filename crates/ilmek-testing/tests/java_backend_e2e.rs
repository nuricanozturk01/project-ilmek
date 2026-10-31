//! End-to-end: the real `ilmek-host` (ILM1 over TLS, real `ProcessLauncher`) runs the real
//! Spring Boot `tasks` backend jar as an OS process (`java -jar`) and a real network client
//! drives it. Requires the jar to be built first:
//!
//!     cd examples/tasks/backend-java && ./gradlew bootJar
//!     cargo test -p ilmek-testing --test java_backend_e2e -- --nocapture

#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]
#![expect(clippy::expect_used, reason = "panicking helpers are fine in tests")]
#![expect(clippy::panic, reason = "a failed precondition aborts the test")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use ilmek_client::{AppClient, Connector};
use ilmek_host::{AppState, BackendRow, Desired, Host, ProcessLauncher, Settings, publish};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{AppRecord, ErrorCode, Version};
use ilmek_testing::{
    Community, Owner, PackageSpec, TestHost, build_package, start_host_with_deadline,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const NAMESPACE: &str = "nuri";
const SLUG: &str = "gorevler";
const JAR_HINT: &str = "build it first: cd examples/tasks/backend-java && ./gradlew bootJar";
const WAIT: Duration = Duration::from_secs(90);

fn jar() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/tasks/backend-java/build/libs/backend-java.jar");
    path.canonicalize().unwrap_or_else(|_| {
        panic!("backend jar missing at {}; {JAR_HINT}", path.display());
    })
}

/// Polls `check` until it returns a value or the deadline passes.
async fn eventually<T, F, Fut>(what: &str, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + WAIT;
    while tokio::time::Instant::now() < deadline {
        if let Some(value) = check().await {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn wait_running(host: &TestHost, app_id: Uuid7) -> Option<u32> {
    eventually("the tasks backend to be running", || async {
        host.store
            .status(app_id)
            .await
            .unwrap()
            .filter(|s| s.state == AppState::Running)
            .map(|s| s.pid)
    })
    .await
}

fn spec<'a>(owner: &Owner, app_id: Uuid7) -> PackageSpec<'a> {
    PackageSpec {
        namespace: NAMESPACE,
        slug: SLUG,
        app_id,
        publisher_id: owner.id,
        version: Version::new(1, 0, 0),
        backend_required: true,
        permissions: vec![Permission::AppCall, Permission::AppSubscribe],
        files: vec![("index.html", b"<html>gorevler</html>".to_vec())],
    }
}

/// Installs the app, points it at `java -jar backend-java.jar`, starts it and publishes it.
async fn deploy(host: &TestHost, owner: &Owner, package: &Path, app_id: Uuid7) -> AppRecord {
    let installed = host.installer.install(package).await.unwrap();
    assert_eq!(installed.app_id, app_id);
    host.store
        .set_backend(
            app_id,
            BackendRow {
                executable: PathBuf::from("java"),
                args: vec!["-jar".into(), jar().display().to_string()],
                accepted_versions: vec![],
            },
        )
        .await
        .unwrap();
    host.store
        .set_desired(app_id, Desired::Running, false)
        .await
        .unwrap();
    let pid = wait_running(host, app_id).await.expect("backend pid");
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let cmdline: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    // The backend must be the real jar running under the JVM, not an in-process stand-in.
    assert!(
        cmdline.iter().any(|a| a == "-jar")
            && cmdline.iter().any(|a| a.ends_with("backend-java.jar")),
        "backend pid {pid} is not running `java -jar backend-java.jar`: {cmdline:?}"
    );
    publish(&owner.member.connector, &host.store, app_id, host.peer_id)
        .await
        .unwrap()
}

async fn member(community: &Community) -> Arc<Connector> {
    Arc::new(community.member().await.connector)
}

fn client(connector: &Arc<Connector>, record: &AppRecord) -> AppClient {
    AppClient::new(Arc::clone(connector), record.clone(), record.version)
}

fn json_of(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("the backend returned JSON")
}

fn task_titles(listed: &[u8]) -> Vec<String> {
    json_of(listed)["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_owned())
        .collect()
}

/// Starts a community, a publisher, and a host that runs backends as real processes.
async fn setup() -> (Community, Owner, TestHost, TempDir) {
    let community = Community::start().await;
    let owner = community.publisher(NAMESPACE).await;
    // Spring Boot startup under parallel load can exceed the default 5 s deadline.
    let host = start_host_with_deadline(&community, Arc::new(ProcessLauncher), 60_000).await;
    let scratch = tempfile::tempdir().unwrap();
    (community, owner, host, scratch)
}

fn package_for(scratch: &TempDir, owner: &Owner, app_id: Uuid7) -> PathBuf {
    build_package(scratch.path(), "tasks", &spec(owner, app_id))
}

#[tokio::test]
async fn plain_read_lists_tasks_from_the_real_backend() {
    let (community, owner, host, scratch) = setup().await;
    let app_id = Uuid7::generate();
    let package = package_for(&scratch, &owner, app_id);
    let record = deploy(&host, &owner, &package, app_id).await;

    let connector = member(&community).await;
    let app = client(&connector, &record);
    let listed = app.read("tasks.list", b"{}").await.unwrap();
    assert_eq!(json_of(&listed.body), json!({ "tasks": [] }));
    assert_eq!(listed.operation_id, None);
}

#[tokio::test]
async fn plain_write_creates_a_task_that_the_next_read_shows() {
    let (community, owner, host, scratch) = setup().await;
    let app_id = Uuid7::generate();
    let package = package_for(&scratch, &owner, app_id);
    let record = deploy(&host, &owner, &package, app_id).await;

    let connector = member(&community).await;
    let app = client(&connector, &record);
    let created = app
        .write("tasks.create", br#"{"title":"sut al"}"#, None)
        .await
        .unwrap();
    let task = json_of(&created.body);
    assert_eq!(task["title"], "sut al");
    assert_eq!(task["done"], false);
    assert!(task["id"].is_i64(), "the backend assigns an integer id");
    assert!(created.operation_id.is_some());

    let listed = app.read("tasks.list", b"{}").await.unwrap();
    assert_eq!(task_titles(&listed.body), vec!["sut al".to_owned()]);
}

#[tokio::test]
async fn identical_retry_returns_the_same_result_and_writes_one_row() {
    let (community, owner, host, scratch) = setup().await;
    let app_id = Uuid7::generate();
    let package = package_for(&scratch, &owner, app_id);
    let record = deploy(&host, &owner, &package, app_id).await;

    let connector = member(&community).await;
    let app = client(&connector, &record);
    let operation = Uuid7::generate();
    let body = br#"{"title":"bir kere"}"#;
    let first = app
        .write("tasks.create", body, Some(operation))
        .await
        .unwrap();
    let again = client(&connector, &record)
        .write("tasks.create", body, Some(operation))
        .await
        .unwrap();
    assert_eq!(
        first, again,
        "a repeat with the same operation id replays the stored result"
    );

    let listed = app.read("tasks.list", b"{}").await.unwrap();
    assert_eq!(task_titles(&listed.body), vec!["bir kere".to_owned()]);
}

#[tokio::test]
async fn reusing_an_operation_id_with_a_different_body_is_a_conflict() {
    let (community, owner, host, scratch) = setup().await;
    let app_id = Uuid7::generate();
    let package = package_for(&scratch, &owner, app_id);
    let record = deploy(&host, &owner, &package, app_id).await;

    let connector = member(&community).await;
    let app = client(&connector, &record);
    let operation = Uuid7::generate();
    app.write("tasks.create", br#"{"title":"ilk"}"#, Some(operation))
        .await
        .unwrap();

    let conflict = app
        .write("tasks.create", br#"{"title":"baska"}"#, Some(operation))
        .await
        .unwrap_err();
    assert_eq!(conflict.remote_code(), Some(ErrorCode::OperationConflict));

    let listed = app.read("tasks.list", b"{}").await.unwrap();
    assert_eq!(task_titles(&listed.body), vec!["ilk".to_owned()]);
}

#[tokio::test]
async fn a_subscriber_receives_the_event_after_a_create() {
    let (community, owner, host, scratch) = setup().await;
    let app_id = Uuid7::generate();
    let package = package_for(&scratch, &owner, app_id);
    let record = deploy(&host, &owner, &package, app_id).await;

    let connector = member(&community).await;
    let app = client(&connector, &record);
    let mut subscription = app.subscribe("tasks.events", None).await.unwrap();

    app.write("tasks.create", br#"{"title":"olay"}"#, None)
        .await
        .unwrap();

    let delivered = tokio::time::timeout(WAIT, subscription.next())
        .await
        .expect("an event arrives before the deadline")
        .unwrap()
        .expect("the stream stays open");
    let event = json_of(&delivered.body);
    assert_eq!(event["type"], "created");
    assert_eq!(event["task"]["title"], "olay");
    subscription.acknowledge(&delivered).await.unwrap();
    subscription.unsubscribe().await.unwrap();
}

#[tokio::test]
async fn a_backend_kill_then_host_restart_keeps_the_write_and_replays_it_once() {
    let (community, owner, host, scratch) = setup().await;
    let app_id = Uuid7::generate();
    let package = package_for(&scratch, &owner, app_id);
    let record = deploy(&host, &owner, &package, app_id).await;

    let connector = member(&community).await;
    let operation = Uuid7::generate();
    let body = br#"{"title":"kill sonrasi"}"#;
    let before = client(&connector, &record)
        .write("tasks.create", body, Some(operation))
        .await
        .unwrap();

    // Kill the real backend process with SIGKILL; the supervisor restarts it.
    let old_pid = wait_running(&host, app_id).await.expect("backend pid");
    let status = Command::new("kill")
        .args(["-KILL", &old_pid.to_string()])
        .status()
        .unwrap();
    assert!(status.success(), "kill -KILL {old_pid} failed");
    eventually("the backend to be restarted with a new pid", || async {
        host.store
            .status(app_id)
            .await
            .unwrap()
            .filter(|s| s.state == AppState::Running && s.pid.is_some_and(|p| p != old_pid))
            .map(|_| ())
    })
    .await;

    // Restart the host itself on the same data directory, then publish again.
    let data_dir = host.data_dir.clone();
    let device_dir = host.member.device_dir().to_path_buf();
    host.running.shutdown().await;
    let port = ilmek_testing::free_port();
    let settings = Settings::parse(&format!(
        "data_dir = \"{}\"\ndevice_dir = \"{}\"\nlisten = \"127.0.0.1:{port}\"\nadvertised = \"127.0.0.1:{port}\"\nreconcile_interval_ms = 50\nbackend_start_deadline_ms = 60000\nheartbeat_interval_ms = 300\nrestart_backoff_ms = 20\n",
        data_dir.display(),
        device_dir.display(),
    ))
    .unwrap();
    let reopened = Host::open(settings, Arc::new(ProcessLauncher), community.clock.clone())
        .await
        .unwrap();
    let store = reopened.store().clone();
    let running = reopened.serve().await.unwrap();
    eventually("the app to run again after the host restart", || async {
        store
            .status(app_id)
            .await
            .unwrap()
            .filter(|s| s.state == AppState::Running)
            .map(|_| ())
    })
    .await;
    eventually("the host to register at its new address", || async {
        (store.host_revision().await.unwrap() >= 2).then_some(())
    })
    .await;
    let moved = publish(&owner.member.connector, &store, app_id, host.peer_id)
        .await
        .unwrap();

    // Replay the same operation id after both the backend kill and the host restart.
    let replay = client(&connector, &moved)
        .write("tasks.create", body, Some(operation))
        .await
        .unwrap();
    assert_eq!(
        replay, before,
        "the replay returns the stored result, not a new task"
    );

    let listed = client(&connector, &moved)
        .read("tasks.list", b"{}")
        .await
        .unwrap();
    assert_eq!(task_titles(&listed.body), vec!["kill sonrasi".to_owned()]);

    running.shutdown().await;
}
