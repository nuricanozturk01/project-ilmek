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

mod over_tcp {
    use std::net::{Ipv4Addr, SocketAddrV4};
    use std::sync::Arc;
    use std::time::Duration;

    use ilmek_core::clock::SystemClock;
    use ilmek_core::discovery::{DiscoveryError, lan_candidates, probe_any, probe_tcp};
    use ilmek_core::listener::{ListenerParams, serve_listener};
    use ilmek_core::session::{Handler, Reply, ServerSessionConfig};
    use ilmek_protocol::Uuid7;
    use ilmek_protocol::wire::{Message, ServerRole};
    use tokio::net::TcpListener;
    use tokio_util::sync::CancellationToken;

    struct Nothing;

    impl Handler for Nothing {
        async fn handle(&self, _request: Message, _reply: Reply) {}
    }

    struct Server {
        address: SocketAddrV4,
        community: Uuid7,
        registry_id: Uuid7,
        stop: CancellationToken,
    }

    async fn serve(role: ServerRole) -> Server {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let std::net::SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            unreachable!("bound to an IPv4 address");
        };
        let (community, registry_id) = (Uuid7::generate(), Uuid7::generate());
        let params = ListenerParams {
            session: Arc::new(ServerSessionConfig::new(
                registry_id,
                role,
                community,
                Arc::new(SystemClock),
            )),
            handler: Arc::new(Nothing),
            max_connections: 8,
        };
        let stop = CancellationToken::new();
        tokio::spawn(serve_listener(listener, params, stop.clone()));
        Server {
            address,
            community,
            registry_id,
            stop,
        }
    }

    #[tokio::test]
    async fn the_community_is_learned_from_the_tcp_port_alone() {
        let registry = serve(ServerRole::Registry).await;
        let profile = probe_tcp(registry.address, Duration::from_secs(3))
            .await
            .unwrap();
        assert_eq!(profile.community_id, registry.community);
        assert_eq!(profile.registry_id, registry.registry_id);
        assert_eq!(profile.registry, registry.address);
        registry.stop.cancel();
    }

    #[tokio::test]
    async fn a_host_or_a_closed_port_is_not_taken_for_a_registry() {
        let host = serve(ServerRole::Host).await;
        let error = probe_tcp(host.address, Duration::from_secs(3))
            .await
            .unwrap_err();
        assert!(
            matches!(error, DiscoveryError::NotARegistry(_)),
            "{error:?}"
        );
        host.stop.cancel();

        let closed = {
            let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let std::net::SocketAddr::V4(address) = socket.local_addr().unwrap() else {
                unreachable!("bound to an IPv4 address");
            };
            address
        };
        let error = probe_tcp(closed, Duration::from_secs(1)).await.unwrap_err();
        assert!(
            matches!(error, DiscoveryError::Unreachable { .. }),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_scan_finds_the_one_registry_among_many_dead_addresses() {
        let registry = serve(ServerRole::Registry).await;
        let mut candidates: Vec<SocketAddrV4> = (1..=40)
            .map(|n| SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, n), 9))
            .collect();
        candidates.insert(17, registry.address);
        let profile = probe_any(
            candidates,
            Duration::from_millis(500),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(profile.community_id, registry.community);
        registry.stop.cancel();

        let nothing = vec![SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9)];
        let error = probe_any(nothing, Duration::from_millis(300), Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(matches!(error, DiscoveryError::NoAnswer(_)), "{error:?}");
    }

    #[test]
    fn a_scan_covers_the_whole_local_network_and_this_machine() {
        let addresses = lan_candidates(Ipv4Addr::new(192, 168, 1, 77), 47401);
        assert_eq!(addresses.len(), 255);
        assert!(addresses.contains(&SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 77), 47401)));
        assert!(addresses.contains(&SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 47401)));
        assert!(addresses.contains(&SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 254), 47401)));
        assert!(addresses.contains(&SocketAddrV4::new(Ipv4Addr::LOCALHOST, 47401)));
    }
}
