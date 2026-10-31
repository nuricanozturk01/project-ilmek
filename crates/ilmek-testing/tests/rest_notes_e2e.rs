//! End-to-end: the real `ilmek-host` runs the plain Spring Boot `rest-notes` jar (no İlmek code in
//! its sources) as an OS process and a real network client drives it through `http.read` and
//! `http.write`. Requires the jar to be built first:
//!
//!     cd examples/rest-notes/backend-java && ./gradlew bootJar
//!     cargo test -p ilmek-testing --test rest_notes_e2e -- --nocapture

#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]
#![expect(clippy::expect_used, reason = "panicking helpers are fine in tests")]
#![expect(clippy::panic, reason = "a failed precondition aborts the test")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ilmek_client::{AppClient, Connector};
use ilmek_host::{AppState, BackendRow, Desired, ProcessLauncher, publish};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{AppRecord, Version};
use ilmek_testing::{
    Community, Owner, PackageSpec, TestHost, build_package, start_host_with_deadline,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const NAMESPACE: &str = "nuri";
const SLUG: &str = "notes-rest";
const JAR_NAME: &str = "rest-notes-backend.jar";
const JAR_HINT: &str = "build it first: cd examples/rest-notes/backend-java && ./gradlew bootJar";
const WAIT: Duration = Duration::from_secs(90);

fn jar() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/rest-notes/backend-java/build/libs")
        .join(JAR_NAME);
    path.canonicalize().unwrap_or_else(|_| {
        panic!("backend jar missing at {}; {JAR_HINT}", path.display());
    })
}

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

fn spec<'a>(owner: &Owner, app_id: Uuid7) -> PackageSpec<'a> {
    PackageSpec {
        namespace: NAMESPACE,
        slug: SLUG,
        app_id,
        publisher_id: owner.id,
        version: Version::new(1, 0, 0),
        backend_required: true,
        permissions: vec![Permission::AppCall],
        files: vec![("index.html", b"<html>notes-rest</html>".to_vec())],
    }
}

struct Deployed {
    community: Community,
    host: TestHost,
    record: AppRecord,
    app_id: Uuid7,
    pid: u32,
    _scratch: TempDir,
}

async fn deploy() -> Deployed {
    let community = Community::start().await;
    let owner = community.publisher(NAMESPACE).await;
    // Spring Boot startup under parallel load can exceed the default 5 s deadline.
    let host = start_host_with_deadline(&community, Arc::new(ProcessLauncher), 60_000).await;
    let scratch = tempfile::tempdir().unwrap();
    let app_id = Uuid7::generate();
    let package = build_package(scratch.path(), "notes-rest", &spec(&owner, app_id));
    let installed = host.installer.install(&package).await.unwrap();
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
    let pid = eventually("the notes backend to be running", || async {
        host.store
            .status(app_id)
            .await
            .unwrap()
            .filter(|s| s.state == AppState::Running)
            .and_then(|s| s.pid)
    })
    .await;
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let cmdline: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    // The backend must be the real jar under the JVM, not an in-process stand-in.
    assert!(
        cmdline.iter().any(|a| a == "-jar") && cmdline.iter().any(|a| a.ends_with(JAR_NAME)),
        "backend pid {pid} is not running `java -jar {JAR_NAME}`: {cmdline:?}"
    );
    let record = publish(&owner.member.connector, &host.store, app_id, host.peer_id)
        .await
        .unwrap();
    Deployed {
        community,
        host,
        record,
        app_id,
        pid,
        _scratch: scratch,
    }
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

fn request(method: &str, path: &str, body: Option<Value>) -> Vec<u8> {
    let mut envelope = json!({ "method": method, "path": path });
    if let Some(body) = body {
        envelope["body"] = body;
    }
    serde_json::to_vec(&envelope).unwrap()
}

async fn read(app: &AppClient, path: &str) -> Value {
    json_of(
        &app.read("http.read", &request("GET", path, None))
            .await
            .unwrap()
            .body,
    )
}

async fn write(app: &AppClient, method: &str, path: &str, body: Option<Value>) -> Value {
    json_of(
        &app.write("http.write", &request(method, path, body), None)
            .await
            .unwrap()
            .body,
    )
}

fn listening_tcp_inodes() -> HashSet<u64> {
    let mut inodes = HashSet::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let text = std::fs::read_to_string(table).unwrap();
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // Field 3 is the state (0A = LISTEN); field 9 is the socket inode.
            if fields.get(3) == Some(&"0A") {
                inodes.insert(fields[9].parse().unwrap());
            }
        }
    }
    inodes
}

fn socket_inodes_of(pid: u32) -> HashSet<u64> {
    std::fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .filter_map(|target| {
            let text = target.to_string_lossy().into_owned();
            text.strip_prefix("socket:[")?
                .strip_suffix(']')?
                .parse()
                .ok()
        })
        .collect()
}

#[tokio::test]
async fn a_plain_rest_controller_serves_crud_over_http_read_and_http_write() {
    let d = deploy().await;
    let connector = member(&d.community).await;
    let app = client(&connector, &d.record);

    let empty = read(&app, "/api/notes").await;
    assert_eq!(empty["status"], 200);
    assert_eq!(empty["body"], json!([]));

    let created = write(
        &app,
        "POST",
        "/api/notes",
        Some(json!({ "title": "milk", "body": "2 litres" })),
    )
    .await;
    assert_eq!(created["status"], 201);
    let id = created["body"]["id"].as_i64().expect("integer id");
    assert_eq!(created["body"]["title"], "milk");
    assert_eq!(created["body"]["body"], "2 litres");
    assert_eq!(created["headers"]["location"], format!("/api/notes/{id}"));

    let one = read(&app, &format!("/api/notes/{id}")).await;
    assert_eq!(one["status"], 200);
    assert_eq!(
        one["body"],
        json!({ "id": id, "title": "milk", "body": "2 litres" })
    );

    let updated = write(
        &app,
        "PUT",
        &format!("/api/notes/{id}"),
        Some(json!({ "title": "oat milk", "body": "1 litre" })),
    )
    .await;
    assert_eq!(updated["status"], 200);
    assert_eq!(
        read(&app, &format!("/api/notes/{id}")).await["body"],
        json!({ "id": id, "title": "oat milk", "body": "1 litre" })
    );

    let invalid = write(
        &app,
        "POST",
        "/api/notes",
        Some(json!({ "title": "  ", "body": "x" })),
    )
    .await;
    assert_eq!(invalid["status"], 400);
    assert_eq!(invalid["body"]["error"], "validation failed");
    assert!(invalid["body"]["fields"]["title"].is_string());
    assert_eq!(
        read(&app, "/api/notes").await["body"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let deleted = write(&app, "DELETE", &format!("/api/notes/{id}"), None).await;
    assert_eq!(deleted["status"], 204);
    let gone = read(&app, &format!("/api/notes/{id}")).await;
    assert_eq!(gone["status"], 404);
    assert!(gone["body"]["error"].is_string());
    assert_eq!(read(&app, "/api/notes").await["body"], json!([]));
    assert_eq!(read(&app, "/api/notes/not-a-number").await["status"], 400);
}

#[tokio::test]
async fn a_retried_write_with_the_same_operation_id_replays_and_creates_one_row() {
    let d = deploy().await;
    let connector = member(&d.community).await;
    let app = client(&connector, &d.record);
    let operation = Uuid7::generate();
    let body = request(
        "POST",
        "/api/notes",
        Some(json!({ "title": "once", "body": "" })),
    );

    let first = app
        .write("http.write", &body, Some(operation))
        .await
        .unwrap();
    let again = client(&connector, &d.record)
        .write("http.write", &body, Some(operation))
        .await
        .unwrap();
    assert_eq!(
        first, again,
        "a repeat with the same operation id replays the stored result"
    );
    assert_eq!(json_of(&first.body)["status"], 201);

    let listed = read(&app, "/api/notes").await;
    let rows = listed["body"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "once");
}

#[tokio::test]
async fn the_backend_jar_process_listens_on_no_tcp_port() {
    let d = deploy().await;
    let connector = member(&d.community).await;
    // Exercise the controller first so a lazily opened listener would exist by now.
    let app = client(&connector, &d.record);
    assert_eq!(read(&app, "/api/notes").await["status"], 200);

    let status = d.host.store.status(d.app_id).await.unwrap().unwrap();
    assert_eq!(status.pid, Some(d.pid), "the backend was not restarted");
    let sockets = socket_inodes_of(d.pid);
    assert!(
        !sockets.is_empty(),
        "the JVM holds at least its IPC socket; the probe reads real fds"
    );
    let listening: Vec<u64> = sockets
        .intersection(&listening_tcp_inodes())
        .copied()
        .collect();
    assert!(
        listening.is_empty(),
        "backend pid {} listens on TCP sockets {listening:?}",
        d.pid
    );
}
