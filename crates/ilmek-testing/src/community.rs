use std::net::Ipv4Addr;
use std::sync::Arc;

use ilmek_client::{Connector, RegistryClient, enroll};
use ilmek_identity::{CommunityProfile, PublisherSigner};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::PeerRole;
use ilmek_registry::{InitOptions, InviteRequest, Registry, RunningRegistry, Settings, initialize};
use tempfile::TempDir;
use time::OffsetDateTime;

use crate::clock::ManualClock;

pub const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;

#[must_use]
pub fn free_port() -> u16 {
    let socket = std::net::TcpListener::bind((LOOPBACK, 0)).unwrap();
    socket.local_addr().unwrap().port()
}

/// A running registry plus helpers to enroll devices into it.
pub struct Community {
    _dir: TempDir,
    pub clock: Arc<ManualClock>,
    pub registry: Arc<Registry>,
    pub running: Option<RunningRegistry>,
    pub profile: CommunityProfile,
    settings: Settings,
}

/// An enrolled device and its connector.
pub struct Member {
    pub connector: Connector,
    pub peer_id: Uuid7,
    dir: TempDir,
}

impl Member {
    #[must_use]
    pub fn device_dir(&self) -> &std::path::Path {
        self.dir.path()
    }
}

/// A publisher: an enrolled device that owns a namespace and a signing key.
pub struct Owner {
    pub member: Member,
    pub id: Uuid7,
    pub signer: PublisherSigner,
    pub key_id: String,
    pub namespace: String,
}

#[must_use]
pub fn invite(roles: &[PeerRole]) -> InviteRequest {
    InviteRequest {
        roles: roles.to_vec(),
        bound_peer: None,
        server_ip: None,
        ttl_seconds: None,
    }
}

impl Community {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::parse(&format!(
            "data_dir = \"{}\"\ncontrol_listen = \"127.0.0.1:{}\"\nenroll_listen = \"127.0.0.1:{}\"\nallowed_endpoints = [\"127.0.0.0/8\", \"10.77.0.0/16\"]\nrevocation_refresh_ms = 50\n",
            dir.path().join("registry").display(),
            free_port(),
            free_port()
        ))
        .unwrap();
        let profile = initialize(
            &settings,
            &InitOptions {
                advertised_ip: LOOPBACK,
                identity_lifetime_days: None,
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        let clock = Arc::new(ManualClock::starting_now());
        let registry = Registry::open(settings.clone(), clock.clone()).unwrap();
        let running = Some(registry.serve().await.unwrap());
        Self {
            _dir: dir,
            clock,
            registry,
            running,
            profile,
            settings,
        }
    }

    pub async fn restart(&mut self) {
        self.running.take().unwrap().shutdown().await;
        self.registry = Registry::open(self.settings.clone(), self.clock.clone()).unwrap();
        self.running = Some(self.registry.serve().await.unwrap());
    }

    pub async fn enroll_with(&self, request: &InviteRequest) -> Member {
        let token = self.registry.create_invite(request).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let device = enroll(&self.profile, &token, "test device", dir.path())
            .await
            .unwrap();
        let peer_id = device.peer_id;
        let connector = Connector::new(device, self.clock.clone()).unwrap();
        let registry = RegistryClient::connect(&connector).await.unwrap();
        registry.refresh_trust(connector.trust()).await.unwrap();
        registry.close().await;
        Member {
            connector,
            peer_id,
            dir,
        }
    }

    pub async fn member(&self) -> Member {
        self.enroll_with(&invite(&[PeerRole::Member])).await
    }

    pub async fn publisher(&self, namespace: &str) -> Owner {
        let member = self
            .enroll_with(&invite(&[PeerRole::Member, PeerRole::Publisher]))
            .await;
        let id = self.registry.add_publisher(namespace).unwrap();
        self.registry
            .add_publisher_device(id, member.peer_id)
            .unwrap();
        self.registry
            .grant_namespace(&namespace.parse().unwrap(), id)
            .unwrap();
        let signer = PublisherSigner::generate().unwrap();
        let key_id = self
            .registry
            .add_publisher_key(id, &signer.public_key())
            .unwrap();
        Owner {
            member,
            id,
            signer,
            key_id,
            namespace: namespace.to_owned(),
        }
    }

    pub async fn host(&self, ip: Ipv4Addr) -> Member {
        self.enroll_with(&InviteRequest {
            roles: vec![PeerRole::Member, PeerRole::Host],
            bound_peer: None,
            server_ip: Some(ip),
            ttl_seconds: None,
        })
        .await
    }
}
