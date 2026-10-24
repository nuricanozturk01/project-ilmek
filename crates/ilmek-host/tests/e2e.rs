#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

mod support;

use std::sync::Arc;

use ilmek_client::{AppClient, PackageCache, fetch_package};
use ilmek_host::{AppState, Desired};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{ErrorCode, Version};
use support::{client_for, deploy, notes_spec, resolve, static_spec, world};
use tokio_util::sync::CancellationToken;

fn code(error: &ilmek_client::Error) -> Option<ErrorCode> {
    error.remote_code()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_static_app_is_published_downloaded_verified_and_cached() {
    let world = world().await;
    let app_id = Uuid7::generate();
    let package = world.package(
        "hello",
        &static_spec(&world.owner, app_id, "merhaba", Version::new(1, 0, 0)),
    );
    let (_, published) = deploy(&world.host, &world.owner, &package, false).await;
    assert_eq!(published.endpoint.port, world.host.address.port());

    let member = world.member().await;
    let record = resolve(&member, "nuri", "merhaba").await;
    assert_eq!(record.app_id, app_id);

    let cache = PackageCache::new(world.scratch.path().join("cache"), 10 * 1024 * 1024);
    let mut last = None;
    let cached = fetch_package(
        &member,
        &record,
        &cache,
        |p| last = Some(p),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(cached.dir.join("assets/app.js")).unwrap(),
        b"console.log('merhaba')"
    );
    assert_eq!(
        std::fs::read(cached.dir.join("assets/css/style.css")).unwrap(),
        b"body{margin:0}"
    );
    let progress = last.unwrap();
    assert_eq!(progress.bytes_done, progress.bytes_total);
    assert_eq!(progress.files_done, 3);

    host_goes_away(&world).await;
    let offline = fetch_package(&member, &record, &cache, |_| {}, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        offline.manifest_hash, record.manifest_hash,
        "a cached, verified package opens without the network"
    );
}

async fn host_goes_away(world: &support::World) {
    world
        .host
        .store
        .set_desired(
            world.host.store.apps().await.unwrap()[0].app_id,
            Desired::Stopped,
            false,
        )
        .await
        .unwrap();
    support::eventually("app to stop", || async {
        world
            .host
            .store
            .apps()
            .await
            .unwrap()
            .iter()
            .all(|a| a.desired == Desired::Stopped)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn notes_are_private_writes_are_exactly_once_and_survive_the_client() {
    let world = world().await;
    let app_id = Uuid7::generate();
    let package = world.package(
        "notes",
        &notes_spec(&world.owner, app_id, Version::new(1, 0, 0)),
    );
    deploy(&world.host, &world.owner, &package, true).await;

    let alice = world.member().await;
    let bob = world.member().await;
    let record = resolve(&alice, "nuri", "notlar").await;
    let alice_app = client_for(&alice, &record);
    let bob_app = client_for(&bob, &record);

    let operation = Uuid7::generate();
    let body = br#"{"text":"secret plan"}"#;
    let first = alice_app
        .write("notes.add", body, Some(operation))
        .await
        .unwrap();
    let again = client_for(&alice, &record)
        .write("notes.add", body, Some(operation))
        .await
        .unwrap();
    assert_eq!(
        first, again,
        "a repeat with the same id returns the stored result"
    );

    let listed = alice_app.read("notes.list", b"{}").await.unwrap();
    let notes: serde_json::Value = serde_json::from_slice(&listed.body).unwrap();
    assert_eq!(
        notes["notes"].as_array().unwrap().len(),
        1,
        "applied once despite two requests"
    );

    let others = bob_app.read("notes.list", b"{}").await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&others.body).unwrap()["notes"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let conflict = alice_app
        .write("notes.add", br#"{"text":"different"}"#, Some(operation))
        .await
        .unwrap_err();
    assert_eq!(code(&conflict), Some(ErrorCode::OperationConflict));
}

#[tokio::test(flavor = "multi_thread")]
async fn data_survives_a_host_restart_and_the_dedupe_record_with_it() {
    let world = world().await;
    let app_id = Uuid7::generate();
    let package = world.package(
        "notes",
        &notes_spec(&world.owner, app_id, Version::new(1, 0, 0)),
    );
    let (_, record) = deploy(&world.host, &world.owner, &package, true).await;
    let member = world.member().await;
    let operation = Uuid7::generate();
    client_for(&member, &record)
        .write("notes.add", br#"{"text":"remember me"}"#, Some(operation))
        .await
        .unwrap();

    let support::World {
        community,
        owner,
        host,
        backends,
        scratch,
    } = world;
    let data_dir = host.data_dir.clone();
    let device_dir = host.member.device_dir().to_path_buf();
    host.running.shutdown().await;

    let (launcher, _handles) = ilmek_testing::notes_launcher();
    let port = ilmek_testing::free_port();
    let settings = ilmek_host::Settings::parse(&format!(
        "data_dir = \"{}\"\ndevice_dir = \"{}\"\nlisten = \"127.0.0.1:{port}\"\nadvertised = \"127.0.0.1:{port}\"\nreconcile_interval_ms = 50\nheartbeat_interval_ms = 300\n",
        data_dir.display(),
        device_dir.display()
    ))
    .unwrap();
    let reopened = ilmek_host::Host::open(settings, Arc::new(launcher), community.clock.clone())
        .await
        .unwrap();
    let store = reopened.store().clone();
    let running = reopened.serve().await.unwrap();
    support::eventually("restarted app", || async {
        store
            .status(app_id)
            .await
            .unwrap()
            .is_some_and(|s| s.state == AppState::Running)
    })
    .await;

    support::eventually("host to re-register at its new address", || async {
        store.host_revision().await.unwrap() >= 2
    })
    .await;
    let moved = ilmek_host::publish(&owner.member.connector, &store, app_id, host.peer_id)
        .await
        .unwrap();
    assert_eq!(
        moved.endpoint.port, port,
        "the registry follows the host to its new address"
    );
    let after = client_for(&member, &moved);
    let listed = after.read("notes.list", b"{}").await.unwrap();
    assert!(String::from_utf8_lossy(&listed.body).contains("remember me"));
    let replay = after
        .write("notes.add", br#"{"text":"remember me"}"#, Some(operation))
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&replay.body).contains("\"id\":1"),
        "dedupe survived the restart"
    );

    let listed = after.read("notes.list", b"{}").await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&listed.body).unwrap()["notes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    running.shutdown().await;
    drop((backends, scratch));
    let _ = AppClient::new;
}
