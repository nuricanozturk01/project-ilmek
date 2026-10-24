#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use ilmek_client::{Error, RegistryClient};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{
    AppRecord, Availability, Endpoint, ErrorCode, Hash, RegisterApp, UnregisterApp, UpdateApp,
    Version,
};
use ilmek_testing::{Community, LOOPBACK, Member, Owner};

fn hash(n: u8) -> Hash {
    Hash::from_bytes([n; 32])
}

fn register(p: &Owner, host: Uuid7, slug: &str, version: Version) -> RegisterApp {
    RegisterApp {
        operation_id: Uuid7::generate(),
        namespace: p.namespace.as_str().parse().unwrap(),
        slug: slug.parse().unwrap(),
        app_id: Uuid7::generate(),
        server_id: host,
        version,
        manifest_hash: hash(1),
        name: slug.parse().unwrap(),
        description: "d".parse().unwrap(),
    }
}

fn update_of(record: &AppRecord, version: Version, manifest: Hash) -> UpdateApp {
    UpdateApp {
        operation_id: Uuid7::generate(),
        app_id: record.app_id,
        expected_revision: record.revision,
        server_id: record.server_id,
        version,
        manifest_hash: manifest,
        name: record.name.clone(),
        description: record.description.clone(),
    }
}

async fn registry_of(member: &Member) -> RegistryClient {
    RegistryClient::connect(&member.connector).await.unwrap()
}

async fn host_ready(community: &Community) -> (Member, Endpoint) {
    let host = community.host(LOOPBACK).await;
    let endpoint = Endpoint {
        ip: LOOPBACK,
        port: 47401,
    };
    registry_of(&host)
        .await
        .host_register(Uuid7::generate(), endpoint, 0)
        .await
        .unwrap();
    (host, endpoint)
}

fn code_of(error: &Error) -> ErrorCode {
    error
        .remote_code()
        .unwrap_or_else(|| unreachable!("not a server error: {error:?}"))
}

#[tokio::test]
async fn a_joined_device_can_use_the_registry_and_unknown_names_are_not_found() {
    let community = Community::start().await;
    let member = community.member().await;
    let registry = registry_of(&member).await;

    let error = registry
        .resolve(&"nuri".parse().unwrap(), &"forum".parse().unwrap())
        .await
        .unwrap_err();
    assert_eq!(code_of(&error), ErrorCode::NotFound);
}

#[tokio::test]
async fn only_the_namespace_owner_can_publish_update_or_unregister() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let rival = community.publisher("ece").await;

    let record = registry_of(&owner.member)
        .await
        .register_app(register(
            &owner,
            host.peer_id,
            "forum",
            Version::new(1, 0, 0),
        ))
        .await
        .unwrap();

    let rival_registry = registry_of(&rival.member).await;
    let mut squat = register(&rival, host.peer_id, "forum", Version::new(1, 0, 0));
    squat.namespace = "nuri".parse().unwrap();
    assert_eq!(
        code_of(&rival_registry.register_app(squat).await.unwrap_err()),
        ErrorCode::Forbidden
    );

    let hijack = update_of(&record, Version::new(2, 0, 0), hash(9));
    assert_eq!(
        code_of(&rival_registry.update_app(hijack).await.unwrap_err()),
        ErrorCode::Forbidden
    );

    let unregister = UnregisterApp {
        operation_id: Uuid7::generate(),
        app_id: record.app_id,
        expected_revision: record.revision,
    };
    assert_eq!(
        code_of(&rival_registry.unregister_app(unregister).await.unwrap_err()),
        ErrorCode::Forbidden
    );
}

#[tokio::test]
async fn names_are_unique_and_a_tombstone_stays_reserved_for_its_owner() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let registry = registry_of(&owner.member).await;

    let first = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    let record = registry.register_app(first.clone()).await.unwrap();
    assert_eq!(record.revision, 1);

    let duplicate = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    assert_eq!(
        code_of(&registry.register_app(duplicate).await.unwrap_err()),
        ErrorCode::NameConflict
    );

    let gone = registry
        .unregister_app(UnregisterApp {
            operation_id: Uuid7::generate(),
            app_id: record.app_id,
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert_eq!(gone.revision, 2);
    assert_eq!(
        code_of(
            &registry
                .resolve(&"nuri".parse().unwrap(), &"forum".parse().unwrap())
                .await
                .unwrap_err()
        ),
        ErrorCode::NotFound
    );

    let squatter = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    assert_eq!(
        code_of(&registry.register_app(squatter).await.unwrap_err()),
        ErrorCode::NameConflict,
        "a different app id may not take a reserved name"
    );

    let mut revived = first;
    revived.operation_id = Uuid7::generate();
    assert_eq!(registry.register_app(revived).await.unwrap().revision, 3);
}

#[tokio::test]
async fn concurrent_duplicate_writes_commit_once_and_conflicting_reuse_is_refused() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let a = registry_of(&owner.member).await;
    let b = registry_of(&owner.member).await;

    let request = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    let (first, second) = tokio::join!(
        a.register_app(request.clone()),
        b.register_app(request.clone())
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(first, second);
    assert_eq!(first.revision, 1);

    let mut altered = request.clone();
    altered.version = Version::new(9, 9, 9);
    assert_eq!(
        code_of(&a.register_app(altered).await.unwrap_err()),
        ErrorCode::OperationConflict
    );

    let renamed_json_order = request;
    assert_eq!(a.register_app(renamed_json_order).await.unwrap(), first);
}

#[tokio::test]
async fn updates_check_revision_and_never_roll_a_version_back() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let registry = registry_of(&owner.member).await;
    let record = registry
        .register_app(register(
            &owner,
            host.peer_id,
            "forum",
            Version::new(1, 1, 0),
        ))
        .await
        .unwrap();

    let stale = UpdateApp {
        expected_revision: 99,
        ..update_of(&record, Version::new(1, 2, 0), hash(2))
    };
    assert_eq!(
        code_of(&registry.update_app(stale).await.unwrap_err()),
        ErrorCode::RevisionConflict
    );

    let downgrade = update_of(&record, Version::new(1, 0, 0), hash(2));
    assert_eq!(
        code_of(&registry.update_app(downgrade).await.unwrap_err()),
        ErrorCode::VersionConflict
    );

    let same_version_new_content = update_of(&record, Version::new(1, 1, 0), hash(2));
    assert_eq!(
        code_of(
            &registry
                .update_app(same_version_new_content)
                .await
                .unwrap_err()
        ),
        ErrorCode::VersionConflict
    );

    let ok = registry
        .update_app(update_of(&record, Version::new(1, 2, 0), hash(2)))
        .await
        .unwrap();
    assert_eq!((ok.revision, ok.version), (2, Version::new(1, 2, 0)));
}

#[tokio::test]
async fn availability_follows_the_host_lease() {
    let community = Community::start().await;
    let (host, endpoint) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let registry = registry_of(&owner.member).await;
    let record = registry
        .register_app(register(
            &owner,
            host.peer_id,
            "forum",
            Version::new(1, 0, 0),
        ))
        .await
        .unwrap();
    assert_eq!(
        (record.availability, record.endpoint),
        (Availability::RecentHeartbeat, endpoint)
    );

    community.clock.advance_ms(91_000);
    let (namespace, slug): (ilmek_protocol::wire::Name, ilmek_protocol::wire::Name) =
        ("nuri".parse().unwrap(), "forum".parse().unwrap());
    let resolve = || registry.resolve(&namespace, &slug);
    assert_eq!(resolve().await.unwrap().availability, Availability::Stale);

    registry_of(&host).await.heartbeat(1).await.unwrap();
    assert_eq!(
        resolve().await.unwrap().availability,
        Availability::RecentHeartbeat
    );

    let stale_revision = registry_of(&host).await.heartbeat(5).await.unwrap_err();
    assert_eq!(code_of(&stale_revision), ErrorCode::RevisionConflict);
}

#[tokio::test]
async fn paging_uses_a_fixed_snapshot_expires_and_belongs_to_one_caller() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let registry = registry_of(&owner.member).await;
    for slug in ["a1", "a2", "a3", "a4", "a5"] {
        registry
            .register_app(register(&owner, host.peer_id, slug, Version::new(1, 0, 0)))
            .await
            .unwrap();
    }

    let (meta, first) = registry.list_apps("", 2, None).await.unwrap();
    assert_eq!(first.records.len(), 2);
    registry
        .register_app(register(&owner, host.peer_id, "a0", Version::new(1, 0, 0)))
        .await
        .unwrap();

    let mut seen: Vec<String> = first.records.iter().map(|r| r.slug.to_string()).collect();
    let mut cursor = meta.next_cursor;
    while let Some(next) = cursor {
        let (meta, page) = registry.list_apps("", 2, Some(next)).await.unwrap();
        seen.extend(page.records.iter().map(|r| r.slug.to_string()));
        cursor = meta.next_cursor;
    }
    assert_eq!(
        seen,
        ["a1", "a2", "a3", "a4", "a5"],
        "no skips or repeats, new entry not mixed in"
    );

    let (meta, _) = registry.list_apps("", 2, None).await.unwrap();
    let stranger = community.member().await;
    let foreign = registry_of(&stranger)
        .await
        .list_apps("", 2, meta.next_cursor.clone())
        .await
        .unwrap_err();
    assert_eq!(code_of(&foreign), ErrorCode::CursorExpired);

    let wrong_limit = registry
        .list_apps("", 3, meta.next_cursor.clone())
        .await
        .unwrap_err();
    assert_eq!(code_of(&wrong_limit), ErrorCode::InvalidMessage);

    community.clock.advance_ms(5 * 60 * 1000 + 1);
    let expired = registry
        .list_apps("", 2, meta.next_cursor)
        .await
        .unwrap_err();
    assert_eq!(code_of(&expired), ErrorCode::CursorExpired);

    let (_, filtered) = registry.list_apps("a3", 10, None).await.unwrap();
    assert_eq!(filtered.records.len(), 1);
    assert_eq!(
        code_of(&registry.list_apps("", 51, None).await.unwrap_err()),
        ErrorCode::InvalidMessage
    );
}

#[tokio::test]
async fn state_survives_a_registry_restart() {
    let mut community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let request = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    let before = registry_of(&owner.member)
        .await
        .register_app(request.clone())
        .await
        .unwrap();

    community.restart().await;

    let registry = registry_of(&owner.member).await;
    let after = registry
        .resolve(&"nuri".parse().unwrap(), &"forum".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(after.app_id, before.app_id);
    assert_eq!(
        registry.register_app(request).await.unwrap().revision,
        1,
        "dedupe record persisted"
    );
}

#[tokio::test]
async fn apps_need_a_registered_host_and_endpoints_must_be_in_allowed_networks() {
    let community = Community::start().await;
    let owner = community.publisher("nuri").await;
    let registry = registry_of(&owner.member).await;

    let unknown_host = register(&owner, Uuid7::generate(), "forum", Version::new(1, 0, 0));
    assert_eq!(
        code_of(&registry.register_app(unknown_host).await.unwrap_err()),
        ErrorCode::InvalidMessage
    );

    let host = community.host(LOOPBACK).await;
    let outside = Endpoint {
        ip: "8.8.8.8".parse().unwrap(),
        port: 47401,
    };
    assert_eq!(
        code_of(
            &registry_of(&host)
                .await
                .host_register(Uuid7::generate(), outside, 0)
                .await
                .unwrap_err()
        ),
        ErrorCode::Forbidden
    );
}

#[tokio::test]
async fn a_host_that_sends_0_0_0_0_is_recorded_under_the_address_it_connected_from() {
    let community = Community::start().await;
    let host = community.host(LOOPBACK).await;
    let owner = community.publisher("nuri").await;
    registry_of(&host)
        .await
        .host_register(
            Uuid7::generate(),
            Endpoint {
                ip: "0.0.0.0".parse().unwrap(),
                port: 47402,
            },
            0,
        )
        .await
        .unwrap();

    let registry = registry_of(&owner.member).await;
    let record = registry
        .register_app(register(
            &owner,
            host.peer_id,
            "forum",
            Version::new(1, 0, 0),
        ))
        .await
        .unwrap();
    assert_eq!(
        record.endpoint,
        Endpoint {
            ip: LOOPBACK,
            port: 47402
        },
        "the connection came from loopback, so that is what was recorded"
    );
}
