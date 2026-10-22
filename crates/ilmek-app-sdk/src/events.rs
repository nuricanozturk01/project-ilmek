//! Durable per-topic event log with opaque, principal-bound cursors (PROTOCOL.md §10).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Cursor, Method};
use ilmek_store::rusqlite::{Connection, OptionalExtension, Transaction, params};
use tokio::sync::Notify;

use crate::error::AppError;

/// Events retained per topic; older cursors answer `CURSOR_EXPIRED`.
pub(crate) const RETAINED_PER_TOPIC: i64 = 10_000;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ilmek_events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    topic TEXT NOT NULL,
    body BLOB NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS ilmek_events_topic ON ilmek_events (topic, seq);
CREATE TABLE IF NOT EXISTS ilmek_event_floor (
    topic TEXT PRIMARY KEY,
    floor_seq INTEGER NOT NULL
) STRICT;
";

/// Appends events inside the caller's business transaction.
#[derive(Debug, Clone, Default)]
pub struct Events {
    pub(crate) notify: Arc<Notify>,
    pending: Arc<AtomicBool>,
}

impl Events {
    /// Adds an event to the log. It becomes visible to subscribers when `tx` commits.
    pub fn append(
        &self,
        tx: &Transaction<'_>,
        topic: &str,
        body: &[u8],
    ) -> Result<Uuid7, AppError> {
        topic
            .parse::<Method>()
            .map_err(|_| AppError::invalid("event topic grammar"))?;
        let event_id = Uuid7::generate();
        tx.execute(
            "INSERT INTO ilmek_events (event_id, topic, body, created_at_ms)
             VALUES (?1, ?2, ?3, CAST(strftime('%s','now') AS INTEGER) * 1000)",
            params![event_id.to_string(), topic, body],
        )?;
        let boundary: Option<i64> = tx
            .query_row(
                "SELECT seq FROM ilmek_events WHERE topic = ?1 ORDER BY seq DESC LIMIT 1 OFFSET ?2",
                params![topic, RETAINED_PER_TOPIC],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(boundary) = boundary {
            tx.execute(
                "DELETE FROM ilmek_events WHERE topic = ?1 AND seq <= ?2",
                params![topic, boundary],
            )?;
            tx.execute(
                "INSERT INTO ilmek_event_floor (topic, floor_seq) VALUES (?1, ?2)
                 ON CONFLICT(topic) DO UPDATE SET floor_seq = MAX(floor_seq, excluded.floor_seq)",
                params![topic, boundary],
            )?;
        }
        self.pending.store(true, Ordering::Relaxed);
        Ok(event_id)
    }

    /// Wakes subscription tasks if anything was appended since the last call.
    pub(crate) fn committed(&self) {
        if self.pending.swap(false, Ordering::Relaxed) {
            self.notify.notify_waiters();
        }
    }

    pub(crate) fn rolled_back(&self) {
        self.pending.store(false, Ordering::Relaxed);
    }
}

pub(crate) struct StoredEvent {
    pub(crate) seq: i64,
    pub(crate) event_id: Uuid7,
    pub(crate) body: Vec<u8>,
}

/// The decoded form of a cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventCursor {
    pub(crate) seq: i64,
    pub(crate) principal: Uuid7,
    pub(crate) topic: String,
}

impl EventCursor {
    pub(crate) fn encode(&self) -> Result<Cursor, AppError> {
        URL_SAFE_NO_PAD
            .encode(format!("v1:{}:{}:{}", self.seq, self.principal, self.topic))
            .parse()
            .map_err(|_| AppError::invalid("cursor"))
    }

    pub(crate) fn decode(cursor: &Cursor) -> Result<Self, AppError> {
        let bad = || AppError::invalid("cursor does not belong to this subscription");
        let raw = URL_SAFE_NO_PAD.decode(cursor.as_str()).map_err(|_| bad())?;
        let text = String::from_utf8(raw).map_err(|_| bad())?;
        let mut parts = text.splitn(4, ':');
        let (Some("v1"), Some(seq), Some(principal), Some(topic)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        Ok(Self {
            seq: seq.parse().map_err(|_| bad())?,
            principal: principal.parse().map_err(|_| bad())?,
            topic: topic.to_owned(),
        })
    }
}

/// Position a new subscription starts after: the last sequence ever assigned.
pub(crate) fn current_boundary(connection: &Connection) -> Result<i64, AppError> {
    let last: Option<i64> = connection
        .query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'ilmek_events'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(last.unwrap_or(0))
}

/// `true` if events after `seq` have been pruned for `topic`.
pub(crate) fn is_expired(connection: &Connection, topic: &str, seq: i64) -> Result<bool, AppError> {
    let floor: Option<i64> = connection
        .query_row(
            "SELECT floor_seq FROM ilmek_event_floor WHERE topic = ?1",
            params![topic],
            |r| r.get(0),
        )
        .optional()?;
    Ok(floor.is_some_and(|floor| seq < floor))
}

pub(crate) fn read_after(
    connection: &Connection,
    topic: &str,
    after: i64,
    limit: i64,
) -> Result<Vec<StoredEvent>, AppError> {
    let mut statement = connection.prepare(
        "SELECT seq, event_id, body FROM ilmek_events WHERE topic = ?1 AND seq > ?2
         ORDER BY seq LIMIT ?3",
    )?;
    let rows = statement.query_map(params![topic, after, limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Vec<u8>>(2)?,
        ))
    })?;
    let mut events = Vec::new();
    for row in rows {
        let (seq, event_id, body) = row?;
        events.push(StoredEvent {
            seq,
            event_id: event_id
                .parse()
                .map_err(|_| AppError::invalid("stored event id"))?,
            body,
        });
    }
    Ok(events)
}
