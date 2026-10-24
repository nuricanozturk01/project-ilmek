mod support;

use std::sync::Arc;
use std::time::Duration;

use ilmek_client::{AppClient, Error, PackageCache, RetryPolicy, fetch_package};
use ilmek_host::{AppState, Desired};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{ErrorCode, Version};
use ilmek_testing::{InProcessLauncher, ManualClock};
use support::{
    client_for, deploy, eventually, notes_spec, resolve, static_spec, wait_state, world,
};
use tokio_util::sync::CancellationToken;

fn no_retry(client: AppClient) -> AppClient {
    client.with_policy(RetryPolicy { delays: vec![] })
}

fn remote(error: &Error) -> Option<ErrorCode> {
    error.remote_code()
}

fn cache(world: &support::World, quota: u64) -> PackageCache {
    PackageCache::new(
        world
            .scratch
            .path()
            .join(format!("cache-{}", Uuid7::generate())),
        quota,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crashing_backend_leaves_other_apps_running_and_recovers() {
    let world = world().await;
    let notes = world.package(
        "notes",
        &notes_spec(&world.owner, Uuid7::generate(), Version::new(1, 0, 0)),
    );
    let (notes_id, notes_record) = deploy(&world.host, &world.owner, &notes, true).await;
    let mut hello_spec = static_spec(
        &world.owner,
        Uuid7::generate(),
        "merhaba",
        Version::new(1, 0, 0),
    );
    hello_spec.namespace = "nuri";
    let hello = world.package("hello", &hello_spec);
    let (_, hello_record) = deploy(&world.host, &world.owner, &hello, false).await;
    let member = world.member().await;
    let notes_client = no_retry(client_for(&member, &notes_record));
    notes_client.read("notes.list", b"{}").await.unwrap();

    world.backends.crash_all();

    let static_cache = cache(&world, 10 << 20);
    fetch_package(
        &member,
        &hello_record,
        &static_cache,
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .expect("the static app is unaffected by the crash");

    eventually(
        "the supervisor to notice and restart the backend",
        || async {
            let status = world.host.store.status(notes_id).await.unwrap().unwrap();
            status.state == AppState::Running && status.restarts >= 1
        },
    )
    .await;
    eventually("calls to work again", || async {
        notes_client.read("notes.list", b"{}").await.is_ok()
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backend_that_keeps_dying_is_abandoned_until_restarted_by_hand() {
    let community = ilmek_testing::Community::start().await;
    let owner = community.publisher("nuri").await;
    let launcher = InProcessLauncher::new(|_| tokio::spawn(async {}));
    let host = ilmek_testing::start_host(&community, Arc::new(launcher)).await;
    let scratch = tempfile::tempdir().unwrap();
    let package = ilmek_testing::build_package(
        scratch.path(),
        "doomed",
        &notes_spec(&owner, Uuid7::generate(), Version::new(1, 0, 0)),
    );
    let installed = host.installer.install(&package).await.unwrap();
    host.store
        .set_backend(
            installed.app_id,
            ilmek_host::BackendRow {
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

    eventually("the host to give up", || async {
        host.store
            .status(installed.app_id)
            .await
            .unwrap()
            .is_some_and(|s| {
                s.state == AppState::Failed && s.last_error.is_some_and(|e| e.contains("gave up"))
            })
    })
    .await;
    let restarts = host
        .store
        .status(installed.app_id)
        .await
        .unwrap()
        .unwrap()
        .restarts;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        host.store
            .status(installed.app_id)
            .await
            .unwrap()
            .unwrap()
            .restarts,
        restarts,
        "no more attempts after giving up"
    );

    host.store
        .set_desired(installed.app_id, Desired::Running, true)
        .await
        .unwrap();
    eventually("a manual restart to try again", || async {
        host.store
            .status(installed.app_id)
            .await
            .unwrap()
            .unwrap()
            .restarts
            > restarts
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_one_app_makes_only_that_app_unavailable() {
    let world = world().await;
    let (a, b) = (Uuid7::generate(), Uuid7::generate());
    let first = world.package(
        "a",
        &static_spec(&world.owner, a, "ilk", Version::new(1, 0, 0)),
    );
    let second = world.package(
        "b",
        &static_spec(&world.owner, b, "ikinci", Version::new(1, 0, 0)),
    );
    let (_, first_record) = deploy(&world.host, &world.owner, &first, false).await;
    let (_, second_record) = deploy(&world.host, &world.owner, &second, false).await;
    let member = world.member().await;

    world
        .host
        .store
        .set_desired(a, Desired::Stopped, false)
        .await
        .unwrap();
    wait_state(&world.host, a, AppState::Stopped).await;

    let stopped = fetch_package(
        &member,
        &first_record,
        &cache(&world, 10 << 20),
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(remote(&stopped), Some(ErrorCode::AppUnavailable));
    fetch_package(
        &member,
        &second_record,
        &cache(&world, 10 << 20),
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_altered_on_the_host_is_rejected_by_the_client_and_never_cached() {
    let world = world().await;
    let app_id = Uuid7::generate();
    let package = world.package(
        "hello",
        &static_spec(&world.owner, app_id, "merhaba", Version::new(1, 0, 0)),
    );
    let (_, record) = deploy(&world.host, &world.owner, &package, false).await;
    let member = world.member().await;

    let version = world
        .host
        .store
        .version(app_id, record.version)
        .await
        .unwrap()
        .unwrap();
    let script = version.directory.join("assets/app.js");
    let original = std::fs::read(&script).unwrap();
    let mut evil = original.clone();
    evil[0] ^= 0x01;
    std::fs::write(&script, &evil).unwrap();

    let client_cache = cache(&world, 10 << 20);
    let error = fetch_package(
        &member,
        &record,
        &client_cache,
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Integrity(_)), "{error:?}");
    assert!(
        client_cache.lookup(&record).unwrap().is_none(),
        "nothing may be activated"
    );

    std::fs::write(&script, &original).unwrap();
    let manifest_path = version.directory.join("manifest.json");
    let mut manifest = std::fs::read(&manifest_path).unwrap();
    manifest.push(b' ');
    std::fs::write(&manifest_path, manifest).unwrap();
    let error = fetch_package(
        &member,
        &record,
        &client_cache,
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, Error::Package(_) | Error::Session(_)),
        "{error:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_downloads_resume_from_the_bytes_already_received() {
    let world = world().await;
    let mut spec = static_spec(
        &world.owner,
        Uuid7::generate(),
        "buyuk",
        Version::new(1, 0, 0),
    );
    let big: Vec<u8> = (0..3_000_000u32)
        .map(|n| u8::try_from(n % 251).unwrap())
        .collect();
    spec.files.push(("data/big.bin", big.clone()));
    let package = world.package("big", &spec);
    let (_, record) = deploy(&world.host, &world.owner, &package, false).await;
    let member = world.member().await;
    let client_cache = cache(&world, 20 << 20);

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let error = fetch_package(
        &member,
        &record,
        &client_cache,
        move |p| {
            if p.bytes_done > 800_000 {
                trigger.cancel();
            }
        },
        &cancel,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled), "{error:?}");
    assert!(client_cache.lookup(&record).unwrap().is_none());

    let mut first_progress = None;
    let done = fetch_package(
        &member,
        &record,
        &client_cache,
        |p| {
            first_progress.get_or_insert(p.bytes_done);
        },
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(
        first_progress.unwrap() > 0,
        "the second attempt starts from what is already on disk"
    );
    assert_eq!(
        std::fs::read(done.dir.join("assets/data/big.bin")).unwrap(),
        big
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cache_quota_refuses_packages_that_do_not_fit() {
    let world = world().await;
    let mut spec = static_spec(
        &world.owner,
        Uuid7::generate(),
        "buyuk",
        Version::new(1, 0, 0),
    );
    spec.files.push(("big.bin", vec![7; 600_000]));
    let package = world.package("big", &spec);
    let (_, record) = deploy(&world.host, &world.owner, &package, false).await;
    let member = world.member().await;

    let error = fetch_package(
        &member,
        &record,
        &cache(&world, 100_000),
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Quota { .. }), "{error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn permissions_versions_and_forged_hashes_are_enforced_by_the_host() {
    let world = world().await;
    let mut spec = notes_spec(&world.owner, Uuid7::generate(), Version::new(1, 0, 0));
    spec.permissions = vec![Permission::AppSubscribe];
    let package = world.package("no-call", &spec);
    let (_, record) = deploy(&world.host, &world.owner, &package, true).await;
    let member = world.member().await;

    let forbidden = no_retry(client_for(&member, &record))
        .read("notes.list", b"{}")
        .await
        .unwrap_err();
    assert_eq!(
        remote(&forbidden),
        Some(ErrorCode::Forbidden),
        "the package never asked for app.call"
    );

    let wrong_version = no_retry(AppClient::new(
        Arc::clone(&member),
        record.clone(),
        Version::new(9, 9, 9),
    ));
    let unsupported = wrong_version.read("notes.list", b"{}").await.unwrap_err();
    assert_eq!(remote(&unsupported), Some(ErrorCode::AppVersionUnsupported));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forged_payload_hash_and_unknown_apps_are_refused() {
    use ilmek_protocol::wire::{AppCall, Hash, Message, Mode, Payload};
    let world = world().await;
    let package = world.package(
        "notes",
        &notes_spec(&world.owner, Uuid7::generate(), Version::new(1, 0, 0)),
    );
    let (_, record) = deploy(&world.host, &world.owner, &package, true).await;
    let member = world.member().await;
    let session = member
        .connect_host(world.host.address, Mode::Control)
        .await
        .unwrap();

    let call = |app_id: Uuid7, hash: Hash| {
        Message::AppCall(Payload {
            data: AppCall {
                app_id,
                client_app_version: record.version,
                method: "notes.list".parse().unwrap(),
                operation_id: None,
                payload_hash: hash,
            },
            body: b"{}".to_vec(),
        })
    };
    let forged = session
        .request(&call(record.app_id, Hash::from_bytes([9; 32])))
        .await
        .unwrap_err();
    assert!(
        matches!(forged, ilmek_core::session::ClientError::Remote(e) if e.code == ErrorCode::HashMismatch)
    );
    let unknown = session
        .request(&call(Uuid7::generate(), Hash::from_bytes([9; 32])))
        .await
        .unwrap_err();
    assert!(
        matches!(unknown, ilmek_core::session::ClientError::Remote(e) if e.code == ErrorCode::NotFound)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn events_reach_subscribers_and_the_per_session_limit_holds() {
    let world = world().await;
    let package = world.package(
        "notes",
        &notes_spec(&world.owner, Uuid7::generate(), Version::new(1, 0, 0)),
    );
    let (_, record) = deploy(&world.host, &world.owner, &package, true).await;
    let member = world.member().await;
    let app = client_for(&member, &record);

    let denied = app.subscribe("admin.secrets", None).await.unwrap_err();
    assert_eq!(remote(&denied), Some(ErrorCode::Forbidden));

    let mut subscription = app.subscribe("notes.events", None).await.unwrap();
    app.write("notes.add", br#"{"text":"first"}"#, None)
        .await
        .unwrap();
    let delivered = tokio::time::timeout(Duration::from_secs(5), subscription.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(delivered.event.sequence, 1);
    assert!(String::from_utf8_lossy(&delivered.body).contains("\"id\":1"));
    subscription.acknowledge(&delivered).await.unwrap();

    let mut held = vec![subscription];
    for _ in 0..7 {
        held.push(app.subscribe("notes.events", None).await.unwrap());
    }
    let ninth = app.subscribe("notes.events", None).await.unwrap_err();
    assert_eq!(remote(&ninth), Some(ErrorCode::ResourceExhausted));

    held[0].unsubscribe().await.unwrap();
    held[0].unsubscribe().await.unwrap();
    app.subscribe("notes.events", None).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn updates_replace_the_published_version_and_failed_updates_change_nothing() {
    let world = world().await;
    let app_id = Uuid7::generate();
    let v1 = world.package(
        "v1",
        &static_spec(&world.owner, app_id, "merhaba", Version::new(1, 0, 0)),
    );
    let (_, first) = deploy(&world.host, &world.owner, &v1, false).await;
    let member = world.member().await;

    let mut spec2 = static_spec(&world.owner, app_id, "merhaba", Version::new(2, 0, 0));
    spec2.files[1].1 = b"console.log('v2')".to_vec();
    let v2 = world.package("v2", &spec2);

    let broken = world.package(
        "broken",
        &static_spec(&world.owner, app_id, "merhaba", Version::new(3, 0, 0)),
    );
    std::fs::write(broken.join("assets/app.js"), "tampered after signing").unwrap();
    assert!(world.host.installer.update(&broken).await.is_err());
    let unchanged = resolve(&member, "nuri", "merhaba").await;
    assert_eq!(
        (unchanged.version, unchanged.revision),
        (first.version, first.revision)
    );

    world.host.installer.update(&v2).await.unwrap();
    let second = ilmek_host::publish(
        &world.owner.member.connector,
        &world.host.store,
        app_id,
        world.host.peer_id,
    )
    .await
    .unwrap();
    assert_eq!(
        (second.version, second.revision),
        (Version::new(2, 0, 0), first.revision + 1)
    );

    let fresh = fetch_package(
        &member,
        &second,
        &cache(&world, 10 << 20),
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(fresh.dir.join("assets/app.js")).unwrap(),
        b"console.log('v2')"
    );
    let old = fetch_package(
        &member,
        &first,
        &cache(&world, 10 << 20),
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(old.dir.join("assets/app.js")).unwrap(),
        b"console.log('merhaba')",
        "old versions stay fetchable"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn names_resolve_from_the_cache_while_the_registry_is_down_but_only_for_known_names() {
    use ilmek_client::{Resolver, Source};
    use ilmek_protocol::address::Address;

    let world = world().await;
    let hello = world.package(
        "hello",
        &static_spec(
            &world.owner,
            Uuid7::generate(),
            "merhaba",
            Version::new(1, 0, 0),
        ),
    );
    deploy(&world.host, &world.owner, &hello, false).await;
    let clock = Arc::new(ManualClock::starting_now());
    let member = Arc::new(world.community.member().await.connector);
    let resolver = Resolver::new(Arc::clone(&member), clock.clone(), None);
    let known: Address = "ilmek://nuri/merhaba".parse().unwrap();
    let unknown: Address = "ilmek://nuri/hic-yok".parse().unwrap();

    let first = resolver.resolve(&known).await.unwrap();
    assert_eq!(first.source, Source::Registry);

    let mut community = world.community;
    community.running.take().unwrap().shutdown().await;
    clock.advance_ms(61_000);

    let offline = resolver.resolve(&known).await.unwrap();
    assert_eq!(
        (offline.source, offline.record.app_id),
        (Source::StaleCache, first.record.app_id)
    );
    assert!(
        resolver.resolve(&unknown).await.is_err(),
        "a name never resolved cannot be guessed offline"
    );

    clock.advance_ms(25 * 60 * 60 * 1000);
    assert!(
        resolver.resolve(&known).await.is_err(),
        "after 24 hours the cached name is not used any more"
    );
}

/// P23: a large transfer lives on its own bulk connection. While its reader is stalled (so the
/// host's writer for that connection is backed up) the control connection of the same member
/// still answers promptly, and the transfer then completes intact.
#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_bulk_transfer_does_not_delay_the_control_connection() {
    use std::net::SocketAddr;
    use std::time::Instant;

    use ilmek_protocol::digest::sha256;
    use ilmek_protocol::wire::{GetAsset, GetManifest, Hash, Message, Mode};

    let world = world().await;
    let mut spec = static_spec(
        &world.owner,
        Uuid7::generate(),
        "agir",
        Version::new(1, 0, 0),
    );
    let big: Vec<u8> = (0..24_000_000u32)
        .map(|n| u8::try_from(n % 251).unwrap())
        .collect();
    spec.files.push(("data/big.bin", big.clone()));
    let package = world.package("stall", &spec);
    let (_, record) = deploy(&world.host, &world.owner, &package, false).await;
    let member = world.member().await;
    let address = SocketAddr::new(record.endpoint.ip.into(), record.endpoint.port);

    let bulk = member.connect_host(address, Mode::Bulk).await.unwrap();
    let mut stream = bulk
        .open_stream(&Message::GetAsset(GetAsset {
            app_id: record.app_id,
            version: record.version,
            manifest_hash: record.manifest_hash,
            path: "data/big.bin".parse().unwrap(),
            asset_hash: Hash::from_bytes(sha256(&big)),
            offset: 0,
        }))
        .await
        .unwrap();
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Message::AssetInfo(_))
    ));

    // Stop reading: the host's writer for the bulk connection fills up and blocks.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let control = member.connect_host(address, Mode::Control).await.unwrap();
    let started = Instant::now();
    let reply = control
        .request(&Message::GetManifest(GetManifest {
            app_id: record.app_id,
            version: record.version,
            manifest_hash: record.manifest_hash,
        }))
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert!(matches!(reply, Message::Manifest(_)));
    assert!(
        elapsed < Duration::from_secs(1),
        "the control connection waited {elapsed:?} behind a stalled transfer"
    );

    // The stalled transfer was only paused, not damaged.
    let mut received = 0u64;
    loop {
        match stream.next().await.unwrap() {
            Some(Message::AssetChunk(chunk)) => received += chunk.body.len() as u64,
            Some(Message::AssetEnd(end)) => {
                assert_eq!(end.total_sent, big.len() as u64);
                break;
            }
            other => panic!("unexpected frame {other:?}"),
        }
    }
    assert_eq!(received, big.len() as u64);
}

/// P13, server half: the host refuses asset requests that do not match the manifest it serves.
#[tokio::test(flavor = "multi_thread")]
async fn asset_requests_that_disagree_with_the_manifest_are_refused() {
    use std::net::SocketAddr;

    use ilmek_core::session::ClientError;
    use ilmek_protocol::digest::sha256;
    use ilmek_protocol::wire::{GetAsset, Hash, Message, Mode};

    let world = world().await;
    let mut spec = static_spec(
        &world.owner,
        Uuid7::generate(),
        "dosya",
        Version::new(1, 0, 0),
    );
    let content = vec![9u8; 5_000];
    spec.files.push(("data/file.bin", content.clone()));
    let package = world.package("assets", &spec);
    let (_, record) = deploy(&world.host, &world.owner, &package, false).await;
    let member = world.member().await;
    let address = SocketAddr::new(record.endpoint.ip.into(), record.endpoint.port);

    let good = GetAsset {
        app_id: record.app_id,
        version: record.version,
        manifest_hash: record.manifest_hash,
        path: "data/file.bin".parse().unwrap(),
        asset_hash: Hash::from_bytes(sha256(&content)),
        offset: 0,
    };
    let refused = |request: GetAsset| {
        let member = &member;
        async move {
            // A refused transfer ends the bulk connection's single transfer slot, so each case
            // uses its own connection.
            let bulk = member.connect_host(address, Mode::Bulk).await.unwrap();
            let mut stream = bulk.open_stream(&Message::GetAsset(request)).await.unwrap();
            match stream.next().await {
                Err(ClientError::Remote(data)) => data.code,
                other => panic!("expected a refusal, got {other:?}"),
            }
        }
    };

    assert_eq!(
        refused(GetAsset {
            offset: 5_001,
            ..good.clone()
        })
        .await,
        ErrorCode::InvalidMessage
    );
    assert_eq!(
        refused(GetAsset {
            asset_hash: Hash::from_bytes([1; 32]),
            ..good.clone()
        })
        .await,
        ErrorCode::HashMismatch
    );
    assert_eq!(
        refused(GetAsset {
            manifest_hash: Hash::from_bytes([2; 32]),
            ..good.clone()
        })
        .await,
        ErrorCode::VersionConflict
    );
    assert_eq!(
        refused(GetAsset {
            path: "data/other.bin".parse().unwrap(),
            ..good.clone()
        })
        .await,
        ErrorCode::NotFound
    );

    // The same request, unmodified, is served.
    let bulk = member.connect_host(address, Mode::Bulk).await.unwrap();
    let mut stream = bulk.open_stream(&Message::GetAsset(good)).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Message::AssetInfo(_))
    ));
}
