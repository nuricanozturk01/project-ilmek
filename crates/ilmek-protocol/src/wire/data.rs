//! `data` objects of every message. Unknown fields are rejected.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::types::{Cursor, Endpoint, Hash, Method, Name, PackagePath, Text, Token, Version};
use crate::ids::Uuid7;

/// Deserializes a field that must be present on the wire but may be `null`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// Messages whose `data` is the empty object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Control,
    Bulk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerRole {
    Registry,
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoodbyeReason {
    User,
    Shutdown,
    Reconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrainReason {
    RequestLimit,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelResult {
    Cancelled,
    AlreadyFinished,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    RecentHeartbeat,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    NameConflict,
    RevisionConflict,
    VersionConflict,
    AppVersionUnsupported,
    UnsupportedVersion,
    UnsupportedMessage,
    InvalidMessage,
    TooLarge,
    RateLimited,
    ResourceExhausted,
    AppUnavailable,
    OperationConflict,
    OperationPending,
    CursorExpired,
    SlowConsumer,
    HashMismatch,
    Cancelled,
    CannotCancel,
    Timeout,
    ServerDraining,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub version: u64,
    /// Who the client says it is. Nothing verifies this (no authentication in this build).
    pub client_id: Uuid7,
    pub mode: Mode,
    pub client_name: Text<64>,
    pub client_version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloAck {
    pub version: u64,
    pub session_id: Uuid7,
    pub server_id: Uuid7,
    pub server_role: ServerRole,
    pub mode: Mode,
    pub max_inflight_requests: u64,
    pub max_subscriptions: u64,
    pub server_time_ms: u64,
}

/// `PING` and `PONG` payload: 16 random bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nonce {
    pub nonce: Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goodbye {
    pub reason: GoodbyeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drain {
    pub reason: DrainReason,
    pub grace_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRecord {
    pub community_id: Uuid7,
    pub namespace: Name,
    pub slug: Name,
    pub app_id: Uuid7,
    pub publisher_id: Uuid7,
    pub server_id: Uuid7,
    pub endpoint: Endpoint,
    pub version: Version,
    pub manifest_hash: Hash,
    pub name: Text<128>,
    pub description: Text<2048>,
    pub revision: u64,
    pub lease_expires_at_ms: u64,
    pub availability: Availability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolve {
    pub namespace: Name,
    pub slug: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolved {
    pub record: AppRecord,
    pub cache_ttl_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListApps {
    pub query: Text<128>,
    pub limit: u64,
    #[serde(deserialize_with = "present")]
    pub cursor: Option<Cursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppsListed {
    pub snapshot_id: Uuid7,
    #[serde(deserialize_with = "present")]
    pub next_cursor: Option<Cursor>,
}

/// Body of `APPS_LISTED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppsPage {
    pub records: Vec<AppRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostRegister {
    pub operation_id: Uuid7,
    pub endpoint: Endpoint,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostRegistered {
    pub server_id: Uuid7,
    pub revision: u64,
    pub heartbeat_interval_seconds: u64,
    pub lease_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostHeartbeat {
    pub host_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatAck {
    pub lease_expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterApp {
    pub operation_id: Uuid7,
    pub namespace: Name,
    pub slug: Name,
    pub app_id: Uuid7,
    pub server_id: Uuid7,
    pub version: Version,
    pub manifest_hash: Hash,
    pub name: Text<128>,
    pub description: Text<2048>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateApp {
    pub operation_id: Uuid7,
    pub app_id: Uuid7,
    pub expected_revision: u64,
    pub server_id: Uuid7,
    pub version: Version,
    pub manifest_hash: Hash,
    pub name: Text<128>,
    pub description: Text<2048>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnregisterApp {
    pub operation_id: Uuid7,
    pub app_id: Uuid7,
    pub expected_revision: u64,
}

/// `APP_REGISTERED` and `APP_UPDATED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordResult {
    pub record: AppRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppUnregistered {
    pub app_id: Uuid7,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetManifest {
    pub app_id: Uuid7,
    pub version: Version,
    pub manifest_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestInfo {
    pub app_id: Uuid7,
    pub version: Version,
    pub manifest_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppCall {
    pub app_id: Uuid7,
    pub client_app_version: Version,
    pub method: Method,
    #[serde(deserialize_with = "present")]
    pub operation_id: Option<Uuid7>,
    pub payload_hash: Hash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppResult {
    pub app_id: Uuid7,
    #[serde(deserialize_with = "present")]
    pub operation_id: Option<Uuid7>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscribe {
    pub app_id: Uuid7,
    pub client_app_version: Version,
    pub topic: Method,
    #[serde(deserialize_with = "present")]
    pub cursor: Option<Cursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscribed {
    pub subscription_id: Uuid7,
    pub app_id: Uuid7,
    pub topic: Method,
    pub cursor: Cursor,
    pub window: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub subscription_id: Uuid7,
    pub app_id: Uuid7,
    pub topic: Method,
    pub sequence: u64,
    pub event_id: Uuid7,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAck {
    pub subscription_id: Uuid7,
    pub sequence: u64,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAcked {
    pub subscription_id: Uuid7,
    pub sequence: u64,
}

/// `UNSUBSCRIBE` and `UNSUBSCRIBED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionRef {
    pub subscription_id: Uuid7,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAsset {
    pub app_id: Uuid7,
    pub version: Version,
    pub manifest_hash: Hash,
    pub path: PackagePath,
    pub asset_hash: Hash,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetInfo {
    pub app_id: Uuid7,
    pub version: Version,
    pub path: PackagePath,
    pub asset_hash: Hash,
    pub total_size: u64,
    pub offset: u64,
    pub chunk_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetChunk {
    pub sequence: u64,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetEnd {
    pub asset_hash: Hash,
    pub total_sent: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cancel {
    pub target_request_id: Uuid7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelAck {
    pub target_request_id: Uuid7,
    pub result: CancelResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorData {
    pub code: ErrorCode,
    pub message: Text<512>,
    pub retryable: bool,
    #[serde(deserialize_with = "present")]
    pub retry_after_ms: Option<u64>,
    pub details: Map<String, Value>,
}
