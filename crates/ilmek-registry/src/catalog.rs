//! Names, hosts and leases (PROTOCOL.md §8.2, §8.3).

use std::net::Ipv4Addr;
use std::sync::Arc;

use ilmek_protocol::Uuid7;
use ilmek_protocol::frame::MAX_BODY_LEN;
use ilmek_protocol::wire::{
    AppRecord, AppUnregistered, AppsListed, AppsPage, Availability, Endpoint, HeartbeatAck,
    HostHeartbeat, HostRegister, HostRegistered, ListApps, PeerRole, RecordResult, RegisterApp,
    Resolve, Resolved, Token, UnregisterApp, UpdateApp, Version,
};
use ilmek_protocol::wire::{Hash, Name};
use ilmek_store::Unsigned;
use ilmek_store::rusqlite::{
    Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};

use crate::error::RegistryError;
use crate::operations::run_operation;
use crate::peers::{Actor, actor};
use crate::registry::Registry;
use crate::snapshots::{Items, decode_cursor, encode_cursor};

const HEARTBEAT_INTERVAL_SECONDS: u64 = 30;
const LEASE_SECONDS: u64 = 90;
const RESOLVE_TTL_SECONDS: u64 = 60;
const MAX_LIST_LIMIT: u64 = 50;
const BODY_MARGIN: usize = 1024;

const APP_SELECT: &str = "
SELECT a.app_id, a.namespace, a.slug, a.publisher_id, a.server_id, h.ip, h.port, a.version,
       a.manifest_hash, a.publisher_key_id, k.public_key, a.name, a.description, a.revision,
       h.lease_expires_at_ms
FROM apps a
JOIN hosts h ON h.server_id = a.server_id
JOIN publisher_keys k ON k.key_id = a.publisher_key_id";

struct RawApp {
    app_id: String,
    namespace: String,
    slug: String,
    publisher_id: String,
    server_id: String,
    ip: String,
    port: u16,
    version: String,
    manifest_hash: String,
    key_id: String,
    public_key: Vec<u8>,
    name: String,
    description: String,
    revision: u64,
    lease_expires_at_ms: u64,
}

fn raw_app(row: &Row<'_>) -> ilmek_store::rusqlite::Result<RawApp> {
    Ok(RawApp {
        app_id: row.get(0)?,
        namespace: row.get(1)?,
        slug: row.get(2)?,
        publisher_id: row.get(3)?,
        server_id: row.get(4)?,
        ip: row.get(5)?,
        port: row.get(6)?,
        version: row.get(7)?,
        manifest_hash: row.get(8)?,
        key_id: row.get(9)?,
        public_key: row.get(10)?,
        name: row.get(11)?,
        description: row.get(12)?,
        revision: row.get::<_, Unsigned>(13)?.0,
        lease_expires_at_ms: row.get::<_, Unsigned>(14)?.0,
    })
}

fn stored<T, E>(value: Result<T, E>) -> Result<T, RegistryError> {
    value.map_err(|_| RegistryError::Internal("stored record is invalid".into()))
}

impl RawApp {
    fn into_record(self, community_id: Uuid7, now_ms: u64) -> Result<AppRecord, RegistryError> {
        let ip: Ipv4Addr = stored(self.ip.parse())?;
        Ok(AppRecord {
            community_id,
            namespace: stored(self.namespace.parse())?,
            slug: stored(self.slug.parse())?,
            app_id: stored(self.app_id.parse())?,
            publisher_id: stored(self.publisher_id.parse())?,
            server_id: stored(self.server_id.parse())?,
            endpoint: Endpoint {
                ip,
                port: self.port,
            },
            version: stored(self.version.parse())?,
            manifest_hash: stored(self.manifest_hash.parse())?,
            publisher_key_id: stored(self.key_id.parse())?,
            publisher_public_key: Token::new(self.public_key),
            name: stored(self.name.parse())?,
            description: stored(self.description.parse())?,
            revision: self.revision,
            lease_expires_at_ms: self.lease_expires_at_ms,
            availability: if now_ms <= self.lease_expires_at_ms {
                Availability::RecentHeartbeat
            } else {
                Availability::Stale
            },
        })
    }
}

fn record_by_id(
    connection: &Connection,
    app_id: Uuid7,
    community_id: Uuid7,
    now_ms: u64,
) -> Result<AppRecord, RegistryError> {
    let sql = format!("{APP_SELECT} WHERE a.app_id = ?1");
    let raw = connection
        .query_row(&sql, params![app_id.to_string()], raw_app)
        .optional()?
        .ok_or(RegistryError::NotFound)?;
    raw.into_record(community_id, now_ms)
}

fn tx(connection: &mut Connection) -> Result<Transaction<'_>, RegistryError> {
    Ok(connection.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

/// What a publisher device may do for one namespace, host and signing key.
fn authorize_publish(
    transaction: &Transaction<'_>,
    actor: &Actor,
    publisher_id: &str,
    server_id: Uuid7,
    key_id: &Hash,
) -> Result<(), RegistryError> {
    let is_device: bool = transaction.query_row(
        "SELECT EXISTS (SELECT 1 FROM publisher_devices WHERE publisher_id = ?1 AND peer_id = ?2)",
        params![publisher_id, actor.peer_id.to_string()],
        |row| row.get(0),
    )?;
    if !is_device {
        return Err(RegistryError::Forbidden(
            "this device does not publish for that name",
        ));
    }
    let host_granted: bool = transaction.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM publisher_hosts p JOIN hosts h ON h.server_id = p.server_id
             WHERE p.publisher_id = ?1 AND p.server_id = ?2)",
        params![publisher_id, server_id.to_string()],
        |row| row.get(0),
    )?;
    if !host_granted {
        return Err(RegistryError::Forbidden(
            "host not available to this publisher",
        ));
    }
    let key_state: Option<Option<i64>> = transaction
        .query_row(
            "SELECT revoked_at_ms FROM publisher_keys WHERE key_id = ?1 AND publisher_id = ?2",
            params![key_id.to_string(), publisher_id],
            |row| row.get(0),
        )
        .optional()?;
    match key_state {
        None => Err(RegistryError::SignatureInvalid("unknown publisher key")),
        Some(Some(_)) => Err(RegistryError::SignatureInvalid("publisher key revoked")),
        Some(None) => Ok(()),
    }
}

fn namespace_publisher(
    transaction: &Transaction<'_>,
    actor: &Actor,
    namespace: &Name,
) -> Result<String, RegistryError> {
    transaction
        .query_row(
            "SELECT n.publisher_id FROM namespaces n
             JOIN publisher_devices d ON d.publisher_id = n.publisher_id
             WHERE n.namespace = ?1 AND d.peer_id = ?2",
            params![namespace.as_str(), actor.peer_id.to_string()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(RegistryError::Forbidden("namespace is not yours"))
}

struct StoredApp {
    publisher_id: String,
    revision: u64,
    unregistered: bool,
    version: String,
    manifest_hash: String,
}

fn stored_app(transaction: &Transaction<'_>, app_id: Uuid7) -> Result<StoredApp, RegistryError> {
    transaction
        .query_row(
            "SELECT publisher_id, revision, unregistered, version, manifest_hash
             FROM apps WHERE app_id = ?1",
            params![app_id.to_string()],
            |row| {
                Ok(StoredApp {
                    publisher_id: row.get(0)?,
                    revision: row.get::<_, Unsigned>(1)?.0,
                    unregistered: row.get::<_, i64>(2)? != 0,
                    version: row.get(3)?,
                    manifest_hash: row.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or(RegistryError::NotFound)
}

impl Registry {
    fn endpoint_allowed(&self, ip: Ipv4Addr) -> bool {
        self.networks.iter().any(|network| network.contains(ip))
    }

    pub(crate) async fn resolve(
        self: &Arc<Self>,
        peer: Uuid7,
        request: Resolve,
    ) -> Result<Resolved, RegistryError> {
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                actor(connection, peer)?;
                let sql = format!(
                    "{APP_SELECT} WHERE a.namespace = ?1 AND a.slug = ?2 AND a.unregistered = 0"
                );
                let raw = connection
                    .query_row(
                        &sql,
                        params![request.namespace.as_str(), request.slug.as_str()],
                        raw_app,
                    )
                    .optional()?
                    .ok_or(RegistryError::NotFound)?;
                Ok(Resolved {
                    record: raw.into_record(registry.community_id(), registry.clock.now_ms())?,
                    cache_ttl_seconds: RESOLVE_TTL_SECONDS,
                })
            })
            .await
    }

    pub(crate) async fn list_apps(
        self: &Arc<Self>,
        peer: Uuid7,
        request: ListApps,
    ) -> Result<(AppsListed, AppsPage), RegistryError> {
        if !(1..=MAX_LIST_LIMIT).contains(&request.limit) {
            return Err(RegistryError::Invalid("limit must be 1..=50"));
        }
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                actor(connection, peer)?;
                let now = registry.clock.now_ms();
                let (snapshot_id, offset, items) = match &request.cursor {
                    None => {
                        let records =
                            search_apps(connection, &registry, request.query.as_str(), now)?;
                        let items = Items::Apps {
                            query: request.query.as_str().to_owned(),
                            limit: request.limit,
                            records,
                        };
                        let id = registry.snapshots.create(peer, now, items);
                        (id, 0, registry.snapshots.get(id, peer, now)?)
                    }
                    Some(cursor) => {
                        let (id, offset) = decode_cursor(cursor)?;
                        (id, offset, registry.snapshots.get(id, peer, now)?)
                    }
                };
                let Items::Apps {
                    query,
                    limit,
                    records,
                } = items.as_ref()
                else {
                    return Err(RegistryError::CursorExpired);
                };
                if query != request.query.as_str() || *limit != request.limit {
                    return Err(RegistryError::Invalid(
                        "cursor does not match query and limit",
                    ));
                }
                page_of_apps(snapshot_id, offset, usize_from(request.limit), records)
            })
            .await
    }

    pub(crate) async fn host_register(
        self: &Arc<Self>,
        peer: Uuid7,
        request: HostRegister,
    ) -> Result<HostRegistered, RegistryError> {
        if !self.endpoint_allowed(request.endpoint.ip) {
            return Err(RegistryError::Forbidden(
                "endpoint is outside the allowed networks",
            ));
        }
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                let tx = tx(connection)?;
                let caller = actor(&tx, peer)?;
                caller.require(PeerRole::Host)?;
                let now = registry.clock.now_ms();
                let result = run_operation(
                    &tx,
                    now,
                    peer,
                    "HOST_REGISTER",
                    request.operation_id,
                    &request,
                    |tx| {
                        let current: Option<Unsigned> = tx
                            .query_row(
                                "SELECT revision FROM hosts WHERE server_id = ?1",
                                params![peer.to_string()],
                                |row| row.get(0),
                            )
                            .optional()?;
                        if current.map_or(0, |c| c.0) != request.expected_revision {
                            return Err(RegistryError::RevisionConflict);
                        }
                        let revision = request.expected_revision + 1;
                        tx.execute(
                            "INSERT INTO hosts (server_id, ip, port, revision, lease_expires_at_ms)
                             VALUES (?1, ?2, ?3, ?4, ?5)
                             ON CONFLICT(server_id) DO UPDATE SET
                                 ip = excluded.ip, port = excluded.port,
                                 revision = excluded.revision,
                                 lease_expires_at_ms = excluded.lease_expires_at_ms",
                            params![
                                peer.to_string(),
                                request.endpoint.ip.to_string(),
                                request.endpoint.port,
                                Unsigned(revision),
                                Unsigned(now + LEASE_SECONDS * 1000)
                            ],
                        )?;
                        Ok(HostRegistered {
                            server_id: peer,
                            revision,
                            heartbeat_interval_seconds: HEARTBEAT_INTERVAL_SECONDS,
                            lease_seconds: LEASE_SECONDS,
                        })
                    },
                )?;
                tx.commit()?;
                Ok(result)
            })
            .await
    }

    pub(crate) async fn host_heartbeat(
        self: &Arc<Self>,
        peer: Uuid7,
        request: HostHeartbeat,
    ) -> Result<HeartbeatAck, RegistryError> {
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                let tx = tx(connection)?;
                actor(&tx, peer)?.require(PeerRole::Host)?;
                let Unsigned(revision) = tx
                    .query_row(
                        "SELECT revision FROM hosts WHERE server_id = ?1",
                        params![peer.to_string()],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or(RegistryError::NotFound)?;
                if revision != request.host_revision {
                    return Err(RegistryError::RevisionConflict);
                }
                let lease = registry.clock.now_ms() + LEASE_SECONDS * 1000;
                tx.execute(
                    "UPDATE hosts SET lease_expires_at_ms = ?2 WHERE server_id = ?1",
                    params![peer.to_string(), Unsigned(lease)],
                )?;
                tx.commit()?;
                Ok(HeartbeatAck {
                    lease_expires_at_ms: lease,
                })
            })
            .await
    }

    pub(crate) async fn register_app(
        self: &Arc<Self>,
        peer: Uuid7,
        request: RegisterApp,
    ) -> Result<RecordResult, RegistryError> {
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                let tx = tx(connection)?;
                let caller = actor(&tx, peer)?;
                caller.require(PeerRole::Publisher)?;
                let publisher_id = namespace_publisher(&tx, &caller, &request.namespace)?;
                authorize_publish(
                    &tx,
                    &caller,
                    &publisher_id,
                    request.server_id,
                    &request.publisher_key_id,
                )?;
                let now = registry.clock.now_ms();
                let result = run_operation(
                    &tx,
                    now,
                    peer,
                    "REGISTER_APP",
                    request.operation_id,
                    &request,
                    |tx| {
                        let revision = claim_name(tx, &request, &publisher_id, now)?;
                        let record =
                            record_by_id(tx, request.app_id, registry.community_id(), now)?;
                        debug_assert_eq!(record.revision, revision);
                        Ok(RecordResult { record })
                    },
                )?;
                tx.commit()?;
                Ok(result)
            })
            .await
    }

    pub(crate) async fn update_app(
        self: &Arc<Self>,
        peer: Uuid7,
        request: UpdateApp,
    ) -> Result<RecordResult, RegistryError> {
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                let tx = tx(connection)?;
                let caller = actor(&tx, peer)?;
                caller.require(PeerRole::Publisher)?;
                let current = stored_app(&tx, request.app_id)?;
                if current.unregistered {
                    return Err(RegistryError::NotFound);
                }
                authorize_publish(
                    &tx,
                    &caller,
                    &current.publisher_id,
                    request.server_id,
                    &request.publisher_key_id,
                )?;
                let now = registry.clock.now_ms();
                let result = run_operation(
                    &tx,
                    now,
                    peer,
                    "UPDATE_APP",
                    request.operation_id,
                    &request,
                    |tx| {
                        let current = stored_app(tx, request.app_id)?;
                        if current.revision != request.expected_revision {
                            return Err(RegistryError::RevisionConflict);
                        }
                        check_version_progress(&current, &request)?;
                        tx.execute(
                            "UPDATE apps SET server_id = ?2, version = ?3, manifest_hash = ?4,
                                 publisher_key_id = ?5, name = ?6, description = ?7,
                                 revision = revision + 1, updated_at_ms = ?8
                             WHERE app_id = ?1",
                            params![
                                request.app_id.to_string(),
                                request.server_id.to_string(),
                                request.version.to_string(),
                                request.manifest_hash.to_string(),
                                request.publisher_key_id.to_string(),
                                request.name.as_str(),
                                request.description.as_str(),
                                Unsigned(now)
                            ],
                        )?;
                        Ok(RecordResult {
                            record: record_by_id(tx, request.app_id, registry.community_id(), now)?,
                        })
                    },
                )?;
                tx.commit()?;
                Ok(result)
            })
            .await
    }

    pub(crate) async fn unregister_app(
        self: &Arc<Self>,
        peer: Uuid7,
        request: UnregisterApp,
    ) -> Result<AppUnregistered, RegistryError> {
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                let tx = tx(connection)?;
                let caller = actor(&tx, peer)?;
                caller.require(PeerRole::Publisher)?;
                let current = stored_app(&tx, request.app_id)?;
                let owned: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM publisher_devices WHERE publisher_id = ?1 AND peer_id = ?2)",
                    params![current.publisher_id, peer.to_string()],
                    |row| row.get(0),
                )?;
                if !owned {
                    return Err(RegistryError::Forbidden("this device does not publish that app"));
                }
                let now = registry.clock.now_ms();
                let result = run_operation(
                    &tx,
                    now,
                    peer,
                    "UNREGISTER_APP",
                    request.operation_id,
                    &request,
                    |tx| {
                        let current = stored_app(tx, request.app_id)?;
                        if current.unregistered {
                            return Ok(AppUnregistered {
                                app_id: request.app_id,
                                revision: current.revision,
                            });
                        }
                        if current.revision != request.expected_revision {
                            return Err(RegistryError::RevisionConflict);
                        }
                        tx.execute(
                            "UPDATE apps SET unregistered = 1, revision = revision + 1,
                                 updated_at_ms = ?2 WHERE app_id = ?1",
                            params![request.app_id.to_string(), Unsigned(now)],
                        )?;
                        Ok(AppUnregistered {
                            app_id: request.app_id,
                            revision: current.revision + 1,
                        })
                    },
                )?;
                tx.commit()?;
                Ok(result)
            })
            .await
    }
}

/// Inserts a new name or revives the caller's own tombstone; returns the new revision.
fn claim_name(
    transaction: &Transaction<'_>,
    request: &RegisterApp,
    publisher_id: &str,
    now_ms: u64,
) -> Result<u64, RegistryError> {
    let holder: Option<(String, String, bool, u64)> = transaction
        .query_row(
            "SELECT app_id, publisher_id, unregistered, revision FROM apps
             WHERE namespace = ?1 AND slug = ?2",
            params![request.namespace.as_str(), request.slug.as_str()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, Unsigned>(3)?.0,
                ))
            },
        )
        .optional()?;

    let app_id = request.app_id.to_string();
    let server_id = request.server_id.to_string();
    let version = request.version.to_string();
    let manifest_hash = request.manifest_hash.to_string();
    let key_id = request.publisher_key_id.to_string();
    let now = Unsigned(now_ms);
    match holder {
        Some((app_id, owner, true, revision))
            if app_id == request.app_id.to_string() && owner == publisher_id =>
        {
            transaction.execute(
                "UPDATE apps SET server_id = ?2, version = ?3, manifest_hash = ?4,
                     publisher_key_id = ?5, name = ?6, description = ?7, updated_at_ms = ?8,
                     unregistered = 0, revision = revision + 1
                 WHERE app_id = ?1",
                params![
                    app_id,
                    server_id,
                    version,
                    manifest_hash,
                    key_id,
                    request.name.as_str(),
                    request.description.as_str(),
                    now
                ],
            )?;
            Ok(revision + 1)
        }
        Some((_, _, false, _)) => Err(RegistryError::NameConflict("name already registered")),
        Some(_) => Err(RegistryError::NameConflict(
            "name belongs to another application",
        )),
        None => {
            let id_taken: bool = transaction.query_row(
                "SELECT EXISTS (SELECT 1 FROM apps WHERE app_id = ?1)",
                params![app_id],
                |row| row.get(0),
            )?;
            if id_taken {
                return Err(RegistryError::NameConflict(
                    "application id registered under another name",
                ));
            }
            transaction.execute(
                "INSERT INTO apps (app_id, namespace, slug, publisher_id, server_id, version,
                     manifest_hash, publisher_key_id, name, description, revision, updated_at_ms)
                 VALUES (?1, ?9, ?10, ?11, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
                params![
                    app_id,
                    server_id,
                    version,
                    manifest_hash,
                    key_id,
                    request.name.as_str(),
                    request.description.as_str(),
                    now,
                    request.namespace.as_str(),
                    request.slug.as_str(),
                    publisher_id
                ],
            )?;
            Ok(1)
        }
    }
}

fn check_version_progress(current: &StoredApp, request: &UpdateApp) -> Result<(), RegistryError> {
    let existing: Version = stored(current.version.parse())?;
    if request.version < existing {
        return Err(RegistryError::VersionConflict("version may not decrease"));
    }
    if request.version == existing && request.manifest_hash.to_string() != current.manifest_hash {
        return Err(RegistryError::VersionConflict(
            "version already published with other content",
        ));
    }
    Ok(())
}

fn search_apps(
    connection: &Connection,
    registry: &Registry,
    query: &str,
    now_ms: u64,
) -> Result<Vec<AppRecord>, RegistryError> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let sql = format!(
        "{APP_SELECT} WHERE a.unregistered = 0 AND
             (a.namespace LIKE ?1 ESCAPE '\\' OR a.slug LIKE ?1 ESCAPE '\\' OR a.name LIKE ?1 ESCAPE '\\')
         ORDER BY a.namespace, a.slug"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![pattern], raw_app)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?.into_record(registry.community_id(), now_ms)?);
    }
    Ok(records)
}

fn usize_from(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn page_of_apps(
    snapshot_id: Uuid7,
    offset: usize,
    limit: usize,
    records: &[AppRecord],
) -> Result<(AppsListed, AppsPage), RegistryError> {
    let start = offset.min(records.len());
    let mut end = (start + limit).min(records.len());
    loop {
        let page = AppsPage {
            records: records[start..end].to_vec(),
        };
        let size = serde_json::to_vec(&page)
            .map_err(|_| RegistryError::Internal("page serialization".into()))?
            .len();
        if size + BODY_MARGIN <= MAX_BODY_LEN || end - start <= 1 {
            let next_cursor = (end < records.len())
                .then(|| encode_cursor(snapshot_id, end))
                .transpose()?;
            return Ok((
                AppsListed {
                    snapshot_id,
                    next_cursor,
                },
                page,
            ));
        }
        end -= 1;
    }
}
