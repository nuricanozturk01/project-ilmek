use std::sync::Arc;

use ilmek_core::clock::Clock;
use ilmek_core::community::CommunityProfile;
use ilmek_protocol::Uuid7;
use ilmek_store::Database;

use crate::error::RegistryError;
use crate::files::{self, Layout};
use crate::schema::MIGRATIONS;
use crate::settings::{Network, Settings};
use crate::snapshots::Snapshots;

/// Everything the registry needs at run time. Shared behind an `Arc` by all sessions.
pub struct Registry {
    pub(crate) db: Database,
    pub(crate) profile: CommunityProfile,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) networks: Vec<Network>,
    pub(crate) snapshots: Snapshots,
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
        let networks = settings
            .networks()
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let db = Database::open(&layout.database(), MIGRATIONS)?;
        Ok(Arc::new(Self {
            db,
            profile,
            clock,
            networks,
            snapshots: Snapshots::default(),
            settings,
        }))
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
