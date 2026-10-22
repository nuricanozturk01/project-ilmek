//! Operator actions. These run in the CLI process against the registry database; the serving
//! process notices revocations through its periodic refresh (`gate`).

use std::net::Ipv4Addr;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ilmek_identity::PublisherKey;
use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::{hex_lower, sha256};
use ilmek_protocol::wire::{Name, PeerRole};
use ilmek_store::Unsigned;
use ilmek_store::rusqlite::TransactionBehavior;
use ilmek_store::rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::error::RegistryError;
use crate::registry::Registry;

const DEFAULT_INVITE_TTL_SECONDS: u64 = 15 * 60;
const MAX_INVITE_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct InviteRequest {
    pub roles: Vec<PeerRole>,
    /// Set for renewals: the new certificate keeps this peer identity.
    pub bound_peer: Option<Uuid7>,
    /// Required for the `host` role: the address the server certificate must cover.
    pub server_ip: Option<Ipv4Addr>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PeerSummary {
    pub peer_id: Uuid7,
    pub display_name: String,
    pub roles: Vec<PeerRole>,
    pub revoked: bool,
}

fn transaction(connection: &mut Connection) -> Result<Transaction<'_>, RegistryError> {
    Ok(connection.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

fn bump_trust_revision(transaction: &Transaction<'_>) -> Result<(), RegistryError> {
    transaction.execute("UPDATE trust_revision SET revision = revision + 1", [])?;
    Ok(())
}

impl Registry {
    /// Creates a single-use enrollment token and returns it. It is shown once and only its
    /// hash is stored.
    pub fn create_invite(&self, request: &InviteRequest) -> Result<String, RegistryError> {
        if request.roles.is_empty() {
            return Err(RegistryError::Invalid("at least one role is required"));
        }
        if request.roles.contains(&PeerRole::Host) && request.server_ip.is_none() {
            return Err(RegistryError::Invalid("host invites need the server ip"));
        }
        let ttl = request.ttl_seconds.unwrap_or(DEFAULT_INVITE_TTL_SECONDS);
        if ttl == 0 || ttl > MAX_INVITE_TTL_SECONDS {
            return Err(RegistryError::Invalid("invite lifetime out of range"));
        }

        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret)
            .map_err(|_| RegistryError::Internal("system randomness unavailable".into()))?;
        let token = URL_SAFE_NO_PAD.encode(secret);
        let now = self.clock.now_ms();
        let roles = serde_json::to_string(&request.roles)
            .map_err(|_| RegistryError::Internal("roles".into()))?;

        self.with_db(|connection| {
            connection.execute(
                "INSERT INTO enrollment_tokens
                     (token_hash, roles, bound_peer_id, server_ip, expires_at_ms, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    hex_lower(&sha256(&secret)),
                    roles,
                    request.bound_peer.map(|p| p.to_string()),
                    request.server_ip.map(|ip| ip.to_string()),
                    Unsigned(now + ttl * 1000),
                    Unsigned(now)
                ],
            )?;
            Ok(())
        })?;
        Ok(token)
    }

    /// Revokes a device: its certificate joins the trust list and new requests are refused.
    pub fn revoke_peer(&self, peer: Uuid7) -> Result<(), RegistryError> {
        let now = self.clock.now_ms();
        self.with_db(|connection| {
            let tx = transaction(connection)?;
            let serial: Option<String> = tx
                .query_row(
                    "SELECT cert_serial FROM peers WHERE peer_id = ?1",
                    params![peer.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            let serial = serial.ok_or(RegistryError::NotFound)?;
            tx.execute(
                "UPDATE peers SET revoked_at_ms = COALESCE(revoked_at_ms, ?2) WHERE peer_id = ?1",
                params![peer.to_string(), Unsigned(now)],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO revocations (kind, value, revoked_at_ms)
                 VALUES ('certificate', ?1, ?2)",
                params![serial, Unsigned(now)],
            )?;
            bump_trust_revision(&tx)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn add_publisher(&self, name: &str) -> Result<Uuid7, RegistryError> {
        let id = Uuid7::generate();
        let now = self.clock.now_ms();
        self.with_db(|connection| {
            connection.execute(
                "INSERT INTO publishers (publisher_id, name, created_at_ms) VALUES (?1, ?2, ?3)",
                params![id.to_string(), name, Unsigned(now)],
            )?;
            Ok(())
        })?;
        Ok(id)
    }

    pub fn add_publisher_device(&self, publisher: Uuid7, peer: Uuid7) -> Result<(), RegistryError> {
        self.with_db(|connection| {
            connection.execute(
                "INSERT OR IGNORE INTO publisher_devices (publisher_id, peer_id) VALUES (?1, ?2)",
                params![publisher.to_string(), peer.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn add_publisher_key(
        &self,
        publisher: Uuid7,
        key: &PublisherKey,
    ) -> Result<String, RegistryError> {
        let key_id = key.key_id();
        let raw = URL_SAFE_NO_PAD
            .decode(key.to_base64url())
            .map_err(|_| RegistryError::Internal("public key encoding".into()))?;
        self.with_db(|connection| {
            connection.execute(
                "INSERT INTO publisher_keys (key_id, publisher_id, public_key) VALUES (?1, ?2, ?3)",
                params![key_id, publisher.to_string(), raw],
            )?;
            Ok(())
        })?;
        Ok(key_id)
    }

    /// Revokes a publisher signing key; clients stop accepting packages signed with it.
    pub fn revoke_publisher_key(&self, key_id: &str) -> Result<(), RegistryError> {
        let now = self.clock.now_ms();
        self.with_db(|connection| {
            let tx = transaction(connection)?;
            let updated = tx.execute(
                "UPDATE publisher_keys SET revoked_at_ms = COALESCE(revoked_at_ms, ?2)
                 WHERE key_id = ?1",
                params![key_id, Unsigned(now)],
            )?;
            if updated == 0 {
                return Err(RegistryError::NotFound);
            }
            tx.execute(
                "INSERT OR IGNORE INTO revocations (kind, value, revoked_at_ms)
                 VALUES ('publisher_key', ?1, ?2)",
                params![key_id, Unsigned(now)],
            )?;
            bump_trust_revision(&tx)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn grant_namespace(&self, namespace: &Name, publisher: Uuid7) -> Result<(), RegistryError> {
        self.with_db(|connection| {
            let inserted = connection.execute(
                "INSERT OR IGNORE INTO namespaces (namespace, publisher_id) VALUES (?1, ?2)",
                params![namespace.as_str(), publisher.to_string()],
            )?;
            let owner: String = connection.query_row(
                "SELECT publisher_id FROM namespaces WHERE namespace = ?1",
                params![namespace.as_str()],
                |row| row.get(0),
            )?;
            if inserted == 0 && owner != publisher.to_string() {
                return Err(RegistryError::NameConflict(
                    "namespace belongs to another publisher",
                ));
            }
            Ok(())
        })
    }

    pub fn grant_host(&self, publisher: Uuid7, server: Uuid7) -> Result<(), RegistryError> {
        self.with_db(|connection| {
            connection.execute(
                "INSERT OR IGNORE INTO publisher_hosts (publisher_id, server_id) VALUES (?1, ?2)",
                params![publisher.to_string(), server.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn list_peers(&self) -> Result<Vec<PeerSummary>, RegistryError> {
        self.with_db(|connection| {
            let mut statement = connection.prepare(
                "SELECT peer_id, display_name, roles, revoked_at_ms FROM peers ORDER BY created_at_ms",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })?;
            let mut peers = Vec::new();
            for row in rows {
                let (id, display_name, roles, revoked) = row?;
                peers.push(PeerSummary {
                    peer_id: id.parse().map_err(|_| RegistryError::Internal("peer id".into()))?,
                    display_name,
                    roles: serde_json::from_str(&roles)
                        .map_err(|_| RegistryError::Internal("roles".into()))?,
                    revoked: revoked.is_some(),
                });
            }
            Ok(peers)
        })
    }
}
