use std::net::SocketAddr;
use std::sync::Arc;

use ilmek_core::clock::Clock;
use ilmek_host::{Host, Installer, Launcher, RunningHost, Settings, Store};
use ilmek_protocol::Uuid7;
use tempfile::TempDir;

use crate::community::{Community, LOOPBACK, Member, Owner, free_port};

/// A host enrolled in a test community, listening on loopback.
pub struct TestHost {
    pub running: RunningHost,
    pub member: Member,
    pub installer: Installer,
    pub store: Store,
    pub address: SocketAddr,
    pub peer_id: Uuid7,
    pub data_dir: std::path::PathBuf,
    _dir: TempDir,
}

/// Starts a host, grants `owner` the right to publish on it and trusts the owner's key.
pub async fn start_host(
    community: &Community,
    owner: &Owner,
    launcher: Arc<dyn Launcher>,
) -> TestHost {
    let member = community.host(LOOPBACK).await;
    community
        .registry
        .grant_host(owner.id, member.peer_id)
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let data_dir = dir.path().join("host");
    let settings = Settings::parse(&format!(
        "data_dir = \"{}\"\ndevice_dir = \"{}\"\nlisten = \"127.0.0.1:{port}\"\nadvertised = \"127.0.0.1:{port}\"\nreconcile_interval_ms = 50\nbackend_start_deadline_ms = 5000\ntrust_refresh_ms = 300\nheartbeat_interval_ms = 300\n",
        data_dir.display(),
        member.device_dir().display(),
    ))
    .unwrap();
    let clock: Arc<dyn Clock> = community.clock.clone();
    let host = Host::open(settings, launcher, clock).await.unwrap();
    host.store()
        .add_trusted_publisher(&owner.signer.public_key(), community.clock.now_ms())
        .await
        .unwrap();
    let installer = host.installer().clone();
    let store = host.store().clone();
    let peer_id = member.peer_id;
    let running = host.serve().await.unwrap();
    let address = running.local_addr;
    TestHost {
        running,
        member,
        installer,
        store,
        address,
        peer_id,
        data_dir,
        _dir: dir,
    }
}
