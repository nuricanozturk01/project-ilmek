//! LAN discovery, so a device can join without being handed a file.
//!
//! A node answers UDP datagrams on [`DISCOVERY_PORT`]. A device broadcasts a probe and takes the
//! first answer: the community id, the registry's id and port. The registry's address is the
//! source address of the answer, so neither side needs to know its own IP.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use ilmek_protocol::Uuid7;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::time::{Instant, timeout_at};
use tokio_util::sync::CancellationToken;

use crate::community::CommunityProfile;

/// UDP port a node listens on for probes.
pub const DISCOVERY_PORT: u16 = 47400;
const PROBE: &[u8] = b"ILMEK-DISCOVER/1";
const MAX_DATAGRAM: usize = 1024;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("no İlmek node answered; is one running on this network? ({0})")]
    NoAnswer(String),
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

/// Asks one known machine instead of the whole network.
pub async fn discover_at(
    node: Ipv4Addr,
    port: u16,
    wait: Duration,
) -> Result<CommunityProfile, DiscoveryError> {
    probe(&[SocketAddr::from((node, port))], wait).await
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
