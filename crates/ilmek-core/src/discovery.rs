//! Finding a node, so a device can join without being handed a file or a community id.
//!
//! Three ways, all giving the same [`CommunityProfile`]:
//!
//! 1. **UDP beacon.** A node answers datagrams on [`DISCOVERY_PORT`]; a device broadcasts a probe
//!    and takes the first answer. The registry's address is the answer's source address.
//! 2. **TCP scan.** If UDP is blocked (firewall, Wi-Fi client isolation of broadcasts), the device
//!    tries the registry port on every address of its own /24 network.
//! 3. **A known address.** [`probe_tcp`] asks one `ip:port` over plain TCP: the registry answers
//!    a `HELLO` for the wrong community with an error that names its real community. Only the
//!    registry's TCP port has to be reachable; UDP is not needed.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Mode, ServerRole, Version};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Instant, timeout, timeout_at};
use tokio_util::sync::CancellationToken;

use crate::community::CommunityProfile;
use crate::session::{ClientError, ClientParams, ClientSession};

/// UDP port a node listens on for probes.
pub const DISCOVERY_PORT: u16 = 47400;
/// The registry's usual TCP port; the one a scan tries.
pub const DEFAULT_REGISTRY_PORT: u16 = 47401;
/// How many addresses a scan tries at once.
const SCAN_PARALLELISM: usize = 64;
/// Time one scanned address gets to answer.
const SCAN_PER_ADDRESS: Duration = Duration::from_millis(700);
const PROBE: &[u8] = b"ILMEK-DISCOVER/1";
const MAX_DATAGRAM: usize = 1024;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("no İlmek node answered; is one running on this network? ({0})")]
    NoAnswer(String),
    #[error(
        "{0} is reachable but did not answer like an İlmek registry (another program, or an older İlmek build: restart the node with the same version as this device)"
    )]
    NotARegistry(SocketAddrV4),
    #[error("cannot reach {address}: {reason}")]
    Unreachable {
        address: SocketAddrV4,
        reason: String,
    },
    #[error("discovery socket error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Announcement {
    ilmek: u32,
    community_id: Uuid7,
    registry_id: Uuid7,
    registry_port: u16,
}

/// Answers probes until `shutdown`. A node that cannot bind the port simply cannot be discovered;
/// devices can still be pointed at it with `--registry`.
pub async fn serve_beacon(
    profile: CommunityProfile,
    port: u16,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    let answer = serde_json::to_vec(&Announcement {
        ilmek: 1,
        community_id: profile.community_id,
        registry_id: profile.registry_id,
        registry_port: profile.registry.port(),
    })
    .map_err(io::Error::other)?;
    let mut buffer = [0u8; MAX_DATAGRAM];
    loop {
        let (length, from) = tokio::select! {
            () = shutdown.cancelled() => return Ok(()),
            received = socket.recv_from(&mut buffer) => received?,
        };
        if &buffer[..length] == PROBE {
            // A lost answer is retried by the prober.
            let _ = socket.send_to(&answer, from).await;
        }
    }
}

/// Looks for a node on the local network (and on this machine) and returns the profile to join.
pub async fn discover(wait: Duration) -> Result<CommunityProfile, DiscoveryError> {
    discover_on(DISCOVERY_PORT, wait).await
}

/// Like [`discover`] with an explicit port.
pub async fn discover_on(port: u16, wait: Duration) -> Result<CommunityProfile, DiscoveryError> {
    probe(
        &[
            SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            SocketAddr::from((Ipv4Addr::BROADCAST, port)),
        ],
        wait,
    )
    .await
}

async fn probe(targets: &[SocketAddr], wait: Duration) -> Result<CommunityProfile, DiscoveryError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_broadcast(true)?;
    let deadline = Instant::now() + wait;
    let mut last_error = String::from("no reply");
    let mut buffer = [0u8; MAX_DATAGRAM];
    // Probe a few times: UDP may drop a datagram, and a node may start a moment later.
    let mut next_probe = Instant::now();
    while Instant::now() < deadline {
        if Instant::now() >= next_probe {
            for target in targets {
                if let Err(error) = socket.send_to(PROBE, *target).await {
                    last_error = error.to_string();
                }
            }
            next_probe = Instant::now() + Duration::from_millis(500);
        }
        let until = next_probe.min(deadline);
        let Ok(received) = timeout_at(until, socket.recv_from(&mut buffer)).await else {
            continue;
        };
        let (length, from) = received?;
        let SocketAddr::V4(from) = from else { continue };
        let Ok(found) = serde_json::from_slice::<Announcement>(&buffer[..length]) else {
            continue;
        };
        if found.ilmek != 1 {
            continue;
        }
        return Ok(CommunityProfile::new(
            found.community_id,
            found.registry_id,
            SocketAddrV4::new(*from.ip(), found.registry_port),
        ));
    }
    Err(DiscoveryError::NoAnswer(last_error))
}

/// Asks the registry at `address` which community it serves, over TCP only.
///
/// The connection asks to join a community nobody has (a random id); the registry refuses with an
/// error that carries its own community id, which is what is needed to join.
pub async fn probe_tcp(
    address: SocketAddrV4,
    wait: Duration,
) -> Result<CommunityProfile, DiscoveryError> {
    let unreachable = |reason: String| DiscoveryError::Unreachable { address, reason };
    let attempt = async {
        let stream = TcpStream::connect(address)
            .await
            .map_err(|e| unreachable(e.to_string()))?;
        let params = ClientParams {
            mode: Mode::Control,
            community_id: Uuid7::generate(),
            client_id: Uuid7::generate(),
            expected_role: ServerRole::Registry,
            client_name: "ilmek-discover".to_owned(),
            client_version: Version::new(1, 0, 0),
            request_timeout: wait,
        };
        match ClientSession::open(stream, params).await {
            Err(ClientError::Remote(error)) => {
                let text = |name: &str| error.details.get(name).and_then(|v| v.as_str());
                let (Some(community), Some(server), Some(role)) =
                    (text("community_id"), text("server_id"), text("server_role"))
                else {
                    return Err(DiscoveryError::NotARegistry(address));
                };
                if role != "registry" {
                    return Err(DiscoveryError::NotARegistry(address));
                }
                let (Ok(community), Ok(server)) = (community.parse(), server.parse()) else {
                    return Err(DiscoveryError::NotARegistry(address));
                };
                Ok(CommunityProfile::new(community, server, address))
            }
            Ok(_) => Err(DiscoveryError::NotARegistry(address)),
            Err(other) => Err(unreachable(other.to_string())),
        }
    };
    timeout(wait, attempt)
        .await
        .map_err(|_| unreachable("no answer in time".to_owned()))?
}

/// The addresses of the /24 network `local` is on (including `local`, where a node on this very
/// machine answers), plus loopback, each with `port`.
#[must_use]
pub fn lan_candidates(local: Ipv4Addr, port: u16) -> Vec<SocketAddrV4> {
    let [a, b, c, _] = local.octets();
    let mut addresses = vec![SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)];
    addresses.extend((1..=254).map(|host| SocketAddrV4::new(Ipv4Addr::new(a, b, c, host), port)));
    addresses
}

/// This machine's address on the network it would use to reach private addresses. No packet is
/// sent: connecting a UDP socket only asks the routing table.
#[must_use]
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(address)
            if !address.ip().is_loopback() && !address.ip().is_unspecified() =>
        {
            Some(*address.ip())
        }
        _ => None,
    }
}

/// Tries every candidate (many at once) and returns the first registry that answers.
pub async fn probe_any(
    candidates: Vec<SocketAddrV4>,
    per_address: Duration,
    wait: Duration,
) -> Result<CommunityProfile, DiscoveryError> {
    let permits = Arc::new(Semaphore::new(SCAN_PARALLELISM));
    let mut tasks = JoinSet::new();
    for address in candidates {
        let permits = Arc::clone(&permits);
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await.ok()?;
            probe_tcp(address, per_address).await.ok()
        });
    }
    let search = async {
        while let Some(finished) = tasks.join_next().await {
            if let Ok(Some(profile)) = finished {
                return Some(profile);
            }
        }
        None
    };
    // Dropping the JoinSet aborts the probes that are still running.
    match timeout(wait, search).await {
        Ok(Some(profile)) => Ok(profile),
        _ => Err(DiscoveryError::NoAnswer(
            "no registry on the local network".to_owned(),
        )),
    }
}

/// Finds a node by every means at once (UDP beacon and a TCP scan of the local /24 network) and
/// returns whichever answers first.
pub async fn discover_any(
    udp_port: u16,
    registry_port: u16,
    wait: Duration,
) -> Result<CommunityProfile, DiscoveryError> {
    let mut attempts = JoinSet::new();
    attempts.spawn(async move { discover_on(udp_port, wait).await });
    attempts.spawn(async move {
        let candidates = match local_ipv4() {
            Some(local) => lan_candidates(local, registry_port),
            None => vec![SocketAddrV4::new(Ipv4Addr::LOCALHOST, registry_port)],
        };
        probe_any(candidates, SCAN_PER_ADDRESS, wait).await
    });
    let mut last = DiscoveryError::NoAnswer("nothing answered".to_owned());
    while let Some(finished) = attempts.join_next().await {
        match finished {
            Ok(Ok(profile)) => return Ok(profile),
            Ok(Err(error)) => last = error,
            Err(_) => {}
        }
    }
    Err(last)
}
