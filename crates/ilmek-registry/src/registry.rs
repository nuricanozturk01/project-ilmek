use std::sync::Arc;

use ilmek_core::clock::Clock;
use ilmek_identity::{CertificateAuthority, CommunityProfile, Identity};
use ilmek_protocol::Uuid7;
use ilmek_store::Database;

use crate::error::RegistryError;
use crate::files::{self, Layout};
use crate::limiter::FailureLimiter;
use crate::schema::MIGRATIONS;
use crate::settings::{Network, Settings};
use crate::snapshots::Snapshots;

/// Everything the registry needs at run time. Shared behind an `Arc` by all sessions.
pub struct Registry {
    pub(crate) db: Database,
    pub(crate) ca: CertificateAuthority,
    pub(crate) identity: Identity,
    pub(crate) profile: CommunityProfile,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) networks: Vec<Network>,
    pub(crate) snapshots: Snapshots,
    pub(crate) limiter: FailureLimiter,
    pub(crate) settings: Settings,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("community_id", &self.profile.community_id)
            .finish_non_exhaustive()
    }
}

impl Registry {
    /// Opens an initialized registry directory.
    pub fn open(settings: Settings, clock: Arc<dyn Clock>) -> Result<Arc<Self>, RegistryError> {
        let layout = Layout::new(&settings.data_dir);
        let profile = CommunityProfile::from_json(&files::read(&layout.profile())?)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let ca = CertificateAuthority::from_parts(
            &files::read(&layout.ca_certificate())?,
            &files::read(&layout.ca_key())?,
        )
        .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let identity = Identity {
            chain_der: vec![files::read(&layout.identity_certificate())?],
            key_pkcs8_der: files::read(&layout.identity_key())?,
        };
        let networks = settings
            .networks()
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let db = Database::open(&layout.database(), MIGRATIONS)?;
        Ok(Arc::new(Self {
            db,
            ca,
            identity,
            profile,
            clock,
            networks,
            snapshots: Snapshots::default(),
            limiter: FailureLimiter::default(),
            settings,
        }))
    }

    /// Runs blocking database work; used by operator commands outside the async runtime.
    pub(crate) fn with_db<T>(
        &self,
        work: impl FnOnce(&mut ilmek_store::rusqlite::Connection) -> Result<T, RegistryError>,
    ) -> Result<T, RegistryError> {
        self.db.call_blocking(work)
    }

    #[must_use]
    pub fn profile(&self) -> &CommunityProfile {
        &self.profile
    }

    #[must_use]
    pub fn community_id(&self) -> Uuid7 {
        self.profile.community_id
    }

    #[must_use]
    pub fn registry_id(&self) -> Uuid7 {
        self.profile.registry_id
    }

    #[must_use]
    pub fn database(&self) -> &Database {
        &self.db
    }
}
