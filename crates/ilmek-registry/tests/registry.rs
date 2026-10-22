#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::net::SocketAddr;
use std::time::Duration;

use ilmek_client::{Error, RegistryClient};
use ilmek_core::session::{ClientParams, ClientSession};
use ilmek_core::tls::{ServerTarget, TlsClient};
use ilmek_identity::DeviceKey;
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{
    AppRecord, Availability, Endpoint, Enroll, ErrorCode, Hash, Message, Mode, PeerRole,
    RegisterApp, ServerRole, Token, UnregisterApp, UpdateApp, Version,
};
use ilmek_registry::InviteRequest;
use ilmek_testing::{Community, LOOPBACK, Member, Owner, invite};

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
        publisher_key_id: p.key_id.parse().unwrap(),
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
        publisher_key_id: record.publisher_key_id,
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

async fn raw_enroll_session(community: &Community) -> ClientSession {
    let tls = TlsClient::new(None, community.profile.ca_der()).unwrap();
    let target = ServerTarget {
        address: SocketAddr::V4(community.profile.registry_enroll),
        server_id: community.profile.registry_id,
    };
    let connection = tls.connect(target).await.unwrap();
    ClientSession::open(
        connection.stream,
        ClientParams {
            mode: Mode::Enroll,
            community_id: community.profile.community_id,
            expected_server_id: community.profile.registry_id,
            expected_role: ServerRole::Registry,
            client_name: "raw".into(),
            client_version: Version::new(1, 0, 0),
            request_timeout: Duration::from_secs(5),
        },
    )
    .await
    .unwrap()
}

fn enroll_message(operation: Uuid7, token: &str, csr: &[u8]) -> Message {
    Message::Enroll(Enroll {
        operation_id: operation,
        token: token.parse().unwrap(),
        display_name: "raw".parse().unwrap(),
        csr_der: Token::new(csr.to_vec()),
    })
}

fn code_of(error: &Error) -> ErrorCode {
    error
        .remote_code()
        .unwrap_or_else(|| unreachable!("not a server error: {error:?}"))
}

#[tokio::test]
async fn enrolled_device_can_use_the_registry_and_unknown_names_are_not_found() {
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
async fn a_lost_enrollment_reply_can_be_replayed_but_not_altered() {
    let community = Community::start().await;
    let token = community
        .registry
        .create_invite(&invite(&[PeerRole::Member]))
        .unwrap();
    let csr = DeviceKey::generate().unwrap().csr_der().unwrap();
    let operation = Uuid7::generate();

    let first = raw_enroll_session(&community)
        .await
        .request(&enroll_message(operation, &token, &csr))
        .await
        .unwrap();
    let again = raw_enroll_session(&community)
        .await
        .request(&enroll_message(operation, &token, &csr))
        .await
        .unwrap();
    assert_eq!(
        first, again,
        "a replay must return the very same certificate"
    );

    let other_csr = DeviceKey::generate().unwrap().csr_der().unwrap();
    let altered = raw_enroll_session(&community)
        .await
        .request(&enroll_message(operation, &token, &other_csr))
        .await;
    assert!(
        matches!(altered, Err(ilmek_core::session::ClientError::Remote(e)) if e.code == ErrorCode::Unauthorized)
    );
}

#[tokio::test]
async fn tokens_are_single_use_and_expire() {
    let community = Community::start().await;
    let token = community
        .registry
        .create_invite(&invite(&[PeerRole::Member]))
        .unwrap();
    let csr = DeviceKey::generate().unwrap().csr_der().unwrap();
    raw_enroll_session(&community)
        .await
        .request(&enroll_message(Uuid7::generate(), &token, &csr))
        .await
        .unwrap();

    let reuse = raw_enroll_session(&community)
        .await
        .request(&enroll_message(Uuid7::generate(), &token, &csr))
        .await;
    assert!(
        matches!(reuse, Err(ilmek_core::session::ClientError::Remote(e)) if e.code == ErrorCode::Unauthorized)
    );

    let fresh = community
        .registry
        .create_invite(&invite(&[PeerRole::Member]))
        .unwrap();
    community.clock.advance_ms(16 * 60 * 1000);
    let late = raw_enroll_session(&community)
        .await
        .request(&enroll_message(Uuid7::generate(), &fresh, &csr))
        .await;
    assert!(
        matches!(late, Err(ilmek_core::session::ClientError::Remote(e)) if e.code == ErrorCode::Unauthorized)
    );
}

#[tokio::test]
async fn repeated_bad_tokens_are_rate_limited() {
    let community = Community::start().await;
    let csr = DeviceKey::generate().unwrap().csr_der().unwrap();
    let mut last = None;
    for _ in 0..7 {
        let attempt = raw_enroll_session(&community)
            .await
            .request(&enroll_message(
                Uuid7::generate(),
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                &csr,
            ))
            .await;
        last = Some(attempt);
    }
    assert!(
        matches!(last, Some(Err(ilmek_core::session::ClientError::Remote(e))) if e.code == ErrorCode::RateLimited && e.retry_after_ms.is_some())
    );
}

#[tokio::test]
async fn only_the_namespace_owner_can_publish_update_or_unregister() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    let rival = community.publisher("ece").await;
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
    community
        .registry
        .grant_host(rival.id, host.peer_id)
        .unwrap();

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
async fn publishing_needs_the_publisher_role_a_granted_host_and_a_valid_key() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;

    let registry = registry_of(&owner.member).await;
    let ungranted = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    assert_eq!(
        code_of(&registry.register_app(ungranted).await.unwrap_err()),
        ErrorCode::Forbidden
    );

    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
    let mut wrong_key = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    wrong_key.publisher_key_id = hash(7);
    assert_eq!(
        code_of(&registry.register_app(wrong_key).await.unwrap_err()),
        ErrorCode::SignatureInvalid
    );

    let plain = community.member().await;
    let plain_registry = registry_of(&plain).await;
    let attempt = register(&owner, host.peer_id, "forum", Version::new(1, 0, 0));
    assert_eq!(
        code_of(&plain_registry.register_app(attempt).await.unwrap_err()),
        ErrorCode::Forbidden
    );

    let endpoint = Endpoint {
        ip: LOOPBACK,
        port: 1,
    };
    assert_eq!(
        code_of(
            &plain_registry
                .host_register(Uuid7::generate(), endpoint, 0)
                .await
                .unwrap_err()
        ),
        ErrorCode::Forbidden,
        "only the host role may register endpoints"
    );
    let outside = Endpoint {
        ip: "8.8.8.8".parse().unwrap(),
        port: 47401,
    };
    assert_eq!(
        code_of(
            &registry_of(&host)
                .await
                .host_register(Uuid7::generate(), outside, 1)
                .await
                .unwrap_err()
        ),
        ErrorCode::Forbidden
    );
}

#[tokio::test]
async fn names_are_unique_and_a_tombstone_stays_reserved_for_its_owner() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
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
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
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
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
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
    community
        .registry
        .grant_host(owner.id, host.peer_id)
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
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
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
async fn a_revoked_device_is_cut_off_and_its_certificate_is_published() {
    let community = Community::start().await;
    let victim = community.member().await;
    let observer = community.member().await;
    let live = registry_of(&victim).await;
    assert!(live.list_apps("", 1, None).await.is_ok());

    community.registry.revoke_peer(victim.peer_id).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let after = live.list_apps("", 1, None).await;
    assert!(
        after.is_err(),
        "the open session must not survive revocation"
    );
    assert!(
        RegistryClient::connect(&victim.connector).await.is_err(),
        "new sessions are refused"
    );

    let registry = registry_of(&observer).await;
    registry
        .refresh_trust(observer.connector.trust())
        .await
        .unwrap();
    let serial = victim_serial(&victim);
    assert!(observer.connector.trust().certificate_revoked(&serial));
    assert!(observer.connector.trust().revision().unwrap() >= 2);
}

fn victim_serial(member: &Member) -> String {
    ilmek_identity::certificate_serial_hex(&member.connector.device().identity.chain_der[0])
        .unwrap()
}

#[tokio::test]
async fn a_revoked_publisher_key_is_rejected_and_listed_in_the_trust_snapshot() {
    let community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
    community
        .registry
        .revoke_publisher_key(&owner.key_id)
        .unwrap();

    let registry = registry_of(&owner.member).await;
    let error = registry
        .register_app(register(
            &owner,
            host.peer_id,
            "forum",
            Version::new(1, 0, 0),
        ))
        .await
        .unwrap_err();
    assert_eq!(code_of(&error), ErrorCode::SignatureInvalid);

    registry
        .refresh_trust(owner.member.connector.trust())
        .await
        .unwrap();
    let key: Hash = owner.key_id.parse().unwrap();
    assert!(owner.member.connector.trust().publisher_key_revoked(&key));
}

#[tokio::test]
async fn state_survives_a_registry_restart() {
    let mut community = Community::start().await;
    let (host, _) = host_ready(&community).await;
    let owner = community.publisher("nuri").await;
    community
        .registry
        .grant_host(owner.id, host.peer_id)
        .unwrap();
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
async fn host_invites_without_an_address_and_empty_roles_are_refused() {
    let community = Community::start().await;
    let no_ip = InviteRequest {
        roles: vec![PeerRole::Host],
        bound_peer: None,
        server_ip: None,
        ttl_seconds: None,
    };
    assert!(community.registry.create_invite(&no_ip).is_err());
    assert!(community.registry.create_invite(&invite(&[])).is_err());
}
