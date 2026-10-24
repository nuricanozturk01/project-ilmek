use std::net::SocketAddr;
use std::sync::Arc;

use ilmek_core::clock::Clock;
use ilmek_host::{Host, Installer, Launcher, RunningHost, Settings, Store};
use ilmek_protocol::Uuid7;
use tempfile::TempDir;

use crate::community::{Community, LOOPBACK, Member, START_ATTEMPTS, free_port};

/// A host that joined a test community, listening on loopback.
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

/// Starts a host in the community.
///
/// Binding a probed port can lose a race against another test process, so a failed bind is
/// retried on a fresh port and data directory.
pub async fn start_host(community: &Community, launcher: Arc<dyn Launcher>) -> TestHost {
    let member = community.host(LOOPBACK).await;

    let dir = tempfile::tempdir().unwrap();
    let mut last_error = String::new();
    for attempt in 0..START_ATTEMPTS {
        let port = free_port();
        let data_dir = dir.path().join(format!("host-{attempt}"));
        let settings = Settings::parse(&format!(
            "data_dir = \"{}\"\ndevice_dir = \"{}\"\nlisten = \"127.0.0.1:{port}\"\nadvertised = \"127.0.0.1:{port}\"\nreconcile_interval_ms = 50\nbackend_start_deadline_ms = 5000\nheartbeat_interval_ms = 300\nrestart_backoff_ms = 20\n",
            data_dir.display(),
            member.device_dir().display(),
        ))
        .unwrap();
        let clock: Arc<dyn Clock> = community.clock.clone();
        let host = Host::open(settings, Arc::clone(&launcher), clock)
            .await
            .unwrap();
        let installer = host.installer().clone();
        let store = host.store().clone();
        let peer_id = member.peer_id;
        match host.serve().await {
            Ok(running) => {
                let address = running.local_addr;
                return TestHost {
                    running,
                    member,
                    installer,
                    store,
                    address,
                    peer_id,
                    data_dir,
                    _dir: dir,
                };
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    panic!("the test host could not bind a port: {last_error}");
}
