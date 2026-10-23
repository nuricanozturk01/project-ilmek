//! Certificates last 30 days, so renewal is an operational necessity: a bound invite re-issues a
//! device's certificate under the same identity, and the registry's own server certificate can be
//! replaced. (Expiry itself is covered by the TLS tests in `ilmek-core`.)

use ilmek_client::{Connector, RegistryClient, enroll};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::PeerRole;
use ilmek_registry::{InitOptions, InviteRequest, reissue_identity};
use ilmek_testing::{Community, LOOPBACK};
use time::OffsetDateTime;

fn bound(peer: Uuid7, roles: &[PeerRole]) -> InviteRequest {
    InviteRequest {
        roles: roles.to_vec(),
        bound_peer: Some(peer),
        server_ip: None,
        ttl_seconds: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bound_invite_renews_the_same_identity_with_a_later_expiry() {
    let community = Community::start().await;
    let dir = tempfile::tempdir().unwrap();
    let token = community
        .registry
        .create_invite(&ilmek_testing::invite(&[PeerRole::Member]))
        .unwrap();
    let original = enroll(&community.profile, &token, "phone", dir.path())
        .await
        .unwrap();

    // Certificates are issued at the community clock. TLS validation uses the real clock, and
    // certificates start one minute in the past, so only a small step keeps them valid.
    community.clock.advance_ms(30_000);

    let token = community
        .registry
        .create_invite(&bound(
            original.peer_id,
            &[PeerRole::Member, PeerRole::Publisher],
        ))
        .unwrap();
    let renewed_dir = tempfile::tempdir().unwrap();
    let renewed = enroll(
        &community.profile,
        &token,
        "phone (renewed)",
        renewed_dir.path(),
    )
    .await
    .unwrap();

    assert_eq!(
        renewed.peer_id, original.peer_id,
        "identity must not change"
    );
    assert!(
        renewed.expires_at_ms >= original.expires_at_ms + 29_000,
        "the renewed certificate must expire later"
    );
    assert!(renewed.roles.contains(&PeerRole::Publisher));

    let peers = community.registry.list_peers().unwrap();
    let mine: Vec<_> = peers
        .iter()
        .filter(|p| p.peer_id == original.peer_id)
        .collect();
    assert_eq!(mine.len(), 1, "renewal must not create a second peer");
    assert!(mine[0].roles.contains(&PeerRole::Publisher));
    assert!(!mine[0].revoked);

    let connector = Connector::new(renewed, community.clock.clone()).unwrap();
    let registry = RegistryClient::connect(&connector).await.unwrap();
    registry.refresh_trust(connector.trust()).await.unwrap();
    registry.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn renewal_is_refused_for_revoked_and_unknown_peers_and_tokens_are_single_use() {
    let community = Community::start().await;
    let member = community.member().await;

    community.registry.revoke_peer(member.peer_id).unwrap();
    let token = community
        .registry
        .create_invite(&bound(member.peer_id, &[PeerRole::Member]))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(
        enroll(&community.profile, &token, "revoked", dir.path())
            .await
            .is_err(),
        "a revoked identity must not be renewable"
    );

    let token = community
        .registry
        .create_invite(&bound(Uuid7::generate(), &[PeerRole::Member]))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(
        enroll(&community.profile, &token, "ghost", dir.path())
            .await
            .is_err(),
        "an unknown peer must not be renewable"
    );

    let live = community.member().await;
    let token = community
        .registry
        .create_invite(&bound(live.peer_id, &[PeerRole::Member]))
        .unwrap();
    let first = tempfile::tempdir().unwrap();
    enroll(&community.profile, &token, "first", first.path())
        .await
        .unwrap();
    let second = tempfile::tempdir().unwrap();
    assert!(
        enroll(&community.profile, &token, "again", second.path())
            .await
            .is_err(),
        "a consumed token must not renew a second time"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reissuing_the_registry_identity_keeps_members_connected_after_a_restart() {
    let mut community = Community::start().await;
    let member = community.member().await;
    let certificate = community.settings.data_dir.join("registry.crt.der");
    let before = std::fs::read(&certificate).unwrap();

    reissue_identity(
        &community.settings,
        &InitOptions {
            advertised_ip: LOOPBACK,
            identity_lifetime_days: None,
        },
        OffsetDateTime::now_utc(),
    )
    .unwrap();
    assert_ne!(std::fs::read(&certificate).unwrap(), before);

    community.restart().await;

    let registry = RegistryClient::connect(&member.connector).await.unwrap();
    registry
        .refresh_trust(member.connector.trust())
        .await
        .unwrap();
    registry.close().await;
}
