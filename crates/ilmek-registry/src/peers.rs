use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::PeerRole;
use ilmek_store::rusqlite::{Connection, OptionalExtension, params};

use crate::error::RegistryError;

/// The authenticated caller of a registry operation, as recorded in `peers`.
#[derive(Debug, Clone)]
pub(crate) struct Actor {
    pub(crate) peer_id: Uuid7,
    pub(crate) roles: Vec<PeerRole>,
}

impl Actor {
    pub(crate) fn require(&self, role: PeerRole) -> Result<(), RegistryError> {
        if self.roles.contains(&role) {
            Ok(())
        } else {
            Err(RegistryError::Forbidden("role required for this operation"))
        }
    }
}

/// Loads the caller; unknown or revoked peers are unauthorized.
pub(crate) fn actor(connection: &Connection, peer_id: Uuid7) -> Result<Actor, RegistryError> {
    let row: Option<(String, Option<i64>)> = connection
        .query_row(
            "SELECT roles, revoked_at_ms FROM peers WHERE peer_id = ?1",
            params![peer_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match row {
        Some((roles, None)) => Ok(Actor {
            peer_id,
            roles: serde_json::from_str(&roles)
                .map_err(|_| RegistryError::Internal("stored roles".into()))?,
        }),
        _ => Err(RegistryError::Unauthorized),
    }
}
