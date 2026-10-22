#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]
#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use ilmek_client::{AppClient, Connector, RegistryClient};
use ilmek_host::{AppState, BackendRow, Desired, publish};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{AppRecord, Version};
use ilmek_testing::{Community, Owner, PackageSpec, TestHost, build_package};
use tempfile::TempDir;

pub(crate) async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    unreachable!("timed out waiting for {what}");
}

pub(crate) async fn wait_state(host: &TestHost, app_id: Uuid7, state: AppState) {
    eventually(&format!("app to be {}", state.as_str()), || async {
        host.store
            .status(app_id)
            .await
            .unwrap()
            .is_some_and(|s| s.state == state)
    })
    .await;
}

pub(crate) fn notes_spec<'a>(owner: &Owner, app_id: Uuid7, version: Version) -> PackageSpec<'a> {
    PackageSpec {
        namespace: "nuri",
        slug: "notlar",
        app_id,
        publisher_id: owner.id,
        version,
        backend_required: true,
        permissions: vec![Permission::AppCall, Permission::AppSubscribe],
        files: vec![("index.html", b"<html>notes</html>".to_vec())],
    }
}

pub(crate) fn static_spec<'a>(
    owner: &Owner,
    app_id: Uuid7,
    slug: &'a str,
    version: Version,
) -> PackageSpec<'a> {
    PackageSpec {
        namespace: "nuri",
        slug,
        app_id,
        publisher_id: owner.id,
        version,
        backend_required: false,
        permissions: vec![],
        files: vec![
            (
                "index.html",
                b"<html><script src=\"app.js\"></script></html>".to_vec(),
            ),
            ("app.js", b"console.log('merhaba')".to_vec()),
            ("css/style.css", b"body{margin:0}".to_vec()),
        ],
    }
}

/// Installs a package, optionally configures a backend, starts it and publishes it.
pub(crate) async fn deploy(
    host: &TestHost,
    owner: &Owner,
    package: &std::path::Path,
    with_backend: bool,
) -> (Uuid7, AppRecord) {
    let installed = host.installer.install(package).await.unwrap();
    if with_backend {
        host.store
            .set_backend(
                installed.app_id,
                BackendRow {
                    executable: "in-process".into(),
                    args: vec![],
                    accepted_versions: vec![],
                },
            )
            .await
            .unwrap();
    }
    host.store
        .set_desired(installed.app_id, Desired::Running, false)
        .await
        .unwrap();
    wait_state(host, installed.app_id, AppState::Running).await;
    let record = publish(
        &owner.member.connector,
        &host.store,
        installed.app_id,
        host.peer_id,
    )
    .await
    .unwrap();
    (installed.app_id, record)
}

pub(crate) async fn resolve(member: &Connector, namespace: &str, slug: &str) -> AppRecord {
    let registry = RegistryClient::connect(member).await.unwrap();
    let record = registry
        .resolve(&namespace.parse().unwrap(), &slug.parse().unwrap())
        .await
        .unwrap();
    registry.close().await;
    record
}

pub(crate) fn client_for(member: &Arc<Connector>, record: &AppRecord) -> AppClient {
    AppClient::new(Arc::clone(member), record.clone(), record.version)
}

pub(crate) struct World {
    pub(crate) community: Community,
    pub(crate) owner: Owner,
    pub(crate) host: TestHost,
    pub(crate) backends: ilmek_testing::BackendHandles,
    pub(crate) scratch: TempDir,
}

pub(crate) async fn world() -> World {
    let community = Community::start().await;
    let owner = community.publisher("nuri").await;
    let (launcher, backends) = ilmek_testing::notes_launcher();
    let host = ilmek_testing::start_host(&community, &owner, Arc::new(launcher)).await;
    World {
        community,
        owner,
        host,
        backends,
        scratch: tempfile::tempdir().unwrap(),
    }
}

impl World {
    pub(crate) fn package(&self, name: &str, spec: &PackageSpec<'_>) -> std::path::PathBuf {
        build_package(self.scratch.path(), name, spec, &self.owner.signer)
    }

    /// A fresh enrolled member whose trust snapshot is current.
    pub(crate) async fn member(&self) -> Arc<Connector> {
        Arc::new(self.community.member().await.connector)
    }
}
