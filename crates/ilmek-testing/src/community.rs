use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, PoisonError};

use ilmek_client::{Connector, Device};
use ilmek_core::community::CommunityProfile;
use ilmek_protocol::Uuid7;
use ilmek_registry::{InitOptions, Registry, RunningRegistry, Settings, initialize};
use tempfile::TempDir;

use crate::clock::ManualClock;

pub const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// Free ports can be taken by a parallel test process between probing and binding.
pub(crate) const START_ATTEMPTS: usize = 8;

static HANDED_OUT: Mutex<Vec<u16>> = Mutex::new(Vec::new());

/// A port nobody has bound a moment ago. Never returns the same port twice in one process, which
/// the OS may otherwise do right after the probe socket is closed. Another process can still take
/// the port before it is bound, so callers that bind it retry (see [`Community::start`]).
#[must_use]
pub fn free_port() -> u16 {
    loop {
        let socket = std::net::TcpListener::bind((LOOPBACK, 0)).unwrap();
        let port = socket.local_addr().unwrap().port();
        let mut handed_out = HANDED_OUT.lock().unwrap_or_else(PoisonError::into_inner);
        if !handed_out.contains(&port) {
            handed_out.push(port);
            return port;
        }
    }
}

/// A running registry plus helpers to join devices to it.
pub struct Community {
    _dir: TempDir,
    pub clock: Arc<ManualClock>,
    pub registry: Arc<Registry>,
    pub running: Option<RunningRegistry>,
    pub profile: CommunityProfile,
    pub settings: Settings,
}

/// A device that joined the community, and its connector.
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

/// A publisher: a device that owns a namespace (the first device to register in it).
pub struct Owner {
    pub member: Member,
    pub id: Uuid7,
    pub namespace: String,
}

impl Community {
    pub async fn start() -> Self {
        let mut last_error = String::new();
        for _ in 0..START_ATTEMPTS {
            match Self::try_start().await {
                Ok(community) => return community,
                Err(error) => last_error = error,
            }
        }
        panic!("the test registry could not bind its port: {last_error}");
    }

    async fn try_start() -> Result<Self, String> {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::parse(&format!(
            "data_dir = \"{}\"\nlisten = \"127.0.0.1:{}\"\nallowed_endpoints = [\"127.0.0.0/8\", \"10.77.0.0/16\"]\n",
            dir.path().join("registry").display(),
            free_port(),
        ))
        .unwrap();
        let clock = Arc::new(ManualClock::starting_now());
        let profile = initialize(
            &settings,
            &InitOptions {
                advertised_ip: LOOPBACK,
            },
            ilmek_core::clock::Clock::now_ms(clock.as_ref()),
        )
        .unwrap();
        let registry = Registry::open(settings.clone(), clock.clone()).unwrap();
        let running = Some(registry.serve().await.map_err(|e| e.to_string())?);
        Ok(Self {
            _dir: dir,
            clock,
            registry,
            running,
            profile,
            settings,
        })
    }

    pub async fn restart(&mut self) {
        self.running.take().unwrap().shutdown().await;
        self.registry = Registry::open(self.settings.clone(), self.clock.clone()).unwrap();
        self.running = Some(self.registry.serve().await.unwrap());
    }

    pub async fn member(&self) -> Member {
        let dir = tempfile::tempdir().unwrap();
        let device = Device::join(&self.profile, dir.path()).unwrap();
        let peer_id = device.peer_id;
        Member {
            connector: Connector::new(device),
            peer_id,
            dir,
        }
    }

    pub async fn publisher(&self, namespace: &str) -> Owner {
        let member = self.member().await;
        Owner {
            id: member.peer_id,
            member,
            namespace: namespace.to_owned(),
        }
    }

    pub async fn host(&self, _ip: Ipv4Addr) -> Member {
        self.member().await
    }
}
