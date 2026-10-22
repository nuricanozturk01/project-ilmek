//! `GET_TRUST`: paged revocation snapshots (PROTOCOL.md §8.4).

use std::sync::Arc;

use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{GetTrust, TrustEntries, TrustEntry, TrustKind, TrustPage};
use ilmek_store::Unsigned;

use crate::error::RegistryError;
use crate::peers::actor;
use crate::registry::Registry;
use crate::snapshots::{Items, decode_cursor, encode_cursor};

const MAX_PAGE: u64 = 100;

impl Registry {
    pub(crate) async fn get_trust(
        self: &Arc<Self>,
        peer: Uuid7,
        request: GetTrust,
    ) -> Result<(TrustPage, TrustEntries), RegistryError> {
        if !(1..=MAX_PAGE).contains(&request.limit) {
            return Err(RegistryError::Invalid("limit must be 1..=100"));
        }
        let registry = Arc::clone(self);
        self.db
            .call(move |connection| {
                actor(connection, peer)?;
                let now = registry.clock.now_ms();
                let (snapshot_id, offset) = match &request.cursor {
                    None => {
                        let mut statement = connection.prepare(
                            "SELECT kind, value, revoked_at_ms FROM revocations ORDER BY kind, value",
                        )?;
                        let rows = statement.query_map([], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Unsigned>(2)?.0))
                        })?;
                        let mut entries = Vec::new();
                        for row in rows {
                            let (kind, value, revoked_at_ms) = row?;
                            let kind = if kind == "certificate" {
                                TrustKind::Certificate
                            } else {
                                TrustKind::PublisherKey
                            };
                            entries.push(TrustEntry { kind, value, revoked_at_ms });
                        }
                        let Unsigned(revision) = connection.query_row(
                            "SELECT revision FROM trust_revision",
                            [],
                            |row| row.get(0),
                        )?;
                        let items = Items::Trust {
                            limit: request.limit,
                            revision,
                            generated_at_ms: now,
                            entries,
                        };
                        (registry.snapshots.create(peer, now, items), 0)
                    }
                    Some(cursor) => decode_cursor(cursor)?,
                };
                let items = registry.snapshots.get(snapshot_id, peer, now)?;
                let Items::Trust { limit, revision, generated_at_ms, entries } = items.as_ref() else {
                    return Err(RegistryError::CursorExpired);
                };
                if *limit != request.limit {
                    return Err(RegistryError::Invalid("cursor does not match limit"));
                }
                let start = offset.min(entries.len());
                let end = (start + usize::try_from(*limit).unwrap_or(usize::MAX)).min(entries.len());
                let next_cursor =
                    (end < entries.len()).then(|| encode_cursor(snapshot_id, end)).transpose()?;
                Ok((
                    TrustPage {
                        snapshot_id,
                        revision: *revision,
                        generated_at_ms: *generated_at_ms,
                        next_cursor,
                    },
                    TrustEntries { entries: entries[start..end].to_vec() },
                ))
            })
            .await
    }
}
