use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use ilmek_core::community::CommunityProfile;
use ilmek_protocol::Uuid7;
use ilmek_store::Database;
use ilmek_store::rusqlite::params;

use crate::error::RegistryError;
use crate::files::{self, Layout};
use crate::schema::MIGRATIONS;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy)]
pub struct InitOptions {
    /// The address clients reach the registry on; written into the community profile.
    pub advertised_ip: Ipv4Addr,
}

/// Creates a new community: its id, the database and the profile devices need to join.
pub fn initialize(
    settings: &Settings,
    options: &InitOptions,
    now_ms: u64,
) -> Result<CommunityProfile, RegistryError> {
    let layout = Layout::new(&settings.data_dir);
    files::create_dir(layout.root())?;

    let community_id = Uuid7::generate();
    let registry_id = Uuid7::generate();
    let profile = CommunityProfile::new(
        community_id,
        registry_id,
        endpoint(settings.listen, options.advertised_ip),
    );
    files::write(
        &layout.profile(),
        &profile
            .to_json()
            .map_err(|e| RegistryError::Internal(e.to_string()))?,
    )?;

    let db = Database::open(&layout.database(), MIGRATIONS)?;
    db.call_blocking::<_, RegistryError>(|connection| {
        connection.execute(
            "INSERT INTO community (id, registry_id, created_at_ms) VALUES (?1, ?2, ?3)",
            params![
                community_id.to_string(),
                registry_id.to_string(),
                i64::try_from(now_ms).unwrap_or(0)
            ],
        )?;
        Ok(())
    })?;
    Ok(profile)
}

fn endpoint(listen: SocketAddr, ip: Ipv4Addr) -> SocketAddrV4 {
    SocketAddrV4::new(ip, listen.port())
}
