#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use ilmek_core::community::CommunityProfile;
use ilmek_core::discovery::{DiscoveryError, discover_on, serve_beacon};
use ilmek_protocol::Uuid7;
use tokio_util::sync::CancellationToken;

fn free_udp_port() -> u16 {
    std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn a_device_finds_the_node_and_learns_the_registry_address() {
    let port = free_udp_port();
    let profile = CommunityProfile::new(
        Uuid7::generate(),
        Uuid7::generate(),
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, 47401),
    );
    let shutdown = CancellationToken::new();
    tokio::spawn(serve_beacon(profile.clone(), port, shutdown.clone()));

    let found = discover_on(port, Duration::from_secs(3)).await.unwrap();
    assert_eq!(found.community_id, profile.community_id);
    assert_eq!(found.registry_id, profile.registry_id);
    assert_eq!(
        found.registry,
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, 47401)
    );
    shutdown.cancel();
}

#[tokio::test]
async fn discovery_gives_up_with_a_clear_error_when_nobody_answers() {
    let port = free_udp_port();
    let error = discover_on(port, Duration::from_millis(800))
        .await
        .unwrap_err();
    assert!(matches!(error, DiscoveryError::NoAnswer(_)), "{error:?}");
}
