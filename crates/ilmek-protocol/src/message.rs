//! Metadata envelope and message catalog (PROTOCOL.md §5, §8-§11).

use serde_json::{Map, Value};
use thiserror::Error;

use crate::ids::Uuid7;
use crate::strict_json::{self, JsonError};

macro_rules! message_types {
    ($($variant:ident => $wire:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum MessageType { $($variant),+ }

        impl MessageType {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $wire),+ }
            }

            #[must_use]
            pub fn from_wire(text: &str) -> Option<Self> {
                match text { $($wire => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

message_types! {
    Hello => "HELLO", HelloAck => "HELLO_ACK", Ping => "PING", Pong => "PONG",
    Goodbye => "GOODBYE", GoodbyeAck => "GOODBYE_ACK", Drain => "DRAIN",
    Enroll => "ENROLL", Enrolled => "ENROLLED",
    Resolve => "RESOLVE", Resolved => "RESOLVED",
    ListApps => "LIST_APPS", AppsListed => "APPS_LISTED",
    HostRegister => "HOST_REGISTER", HostRegistered => "HOST_REGISTERED",
    HostHeartbeat => "HOST_HEARTBEAT", HeartbeatAck => "HEARTBEAT_ACK",
    RegisterApp => "REGISTER_APP", AppRegistered => "APP_REGISTERED",
    UnregisterApp => "UNREGISTER_APP", AppUnregistered => "APP_UNREGISTERED",
    UpdateApp => "UPDATE_APP", AppUpdated => "APP_UPDATED",
    GetTrust => "GET_TRUST", TrustPage => "TRUST_PAGE",
    GetManifest => "GET_MANIFEST", Manifest => "MANIFEST",
    AppCall => "APP_CALL", AppResult => "APP_RESULT",
    Subscribe => "SUBSCRIBE", Subscribed => "SUBSCRIBED", Event => "EVENT",
    EventAck => "EVENT_ACK", EventAcked => "EVENT_ACKED",
    Unsubscribe => "UNSUBSCRIBE", Unsubscribed => "UNSUBSCRIBED",
    GetAsset => "GET_ASSET", AssetInfo => "ASSET_INFO",
    AssetChunk => "ASSET_CHUNK", AssetEnd => "ASSET_END",
    Cancel => "CANCEL", CancelAck => "CANCEL_ACK", Error => "ERROR",
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnvelopeError {
    #[error(transparent)]
    Json(#[from] JsonError),
    #[error("envelope must contain exactly type, request_id, community_id and data")]
    Fields,
    #[error("unsupported message type `{0}`")]
    UnsupportedType(String),
    #[error("field `{0}` is not a canonical UUIDv7")]
    Uuid(&'static str),
    #[error("`data` must be an object")]
    Data,
}

/// The four-field metadata object before the `type` string is interpreted.
#[derive(Debug, Clone, PartialEq)]
pub struct RawEnvelope {
    pub type_name: String,
    pub request_id: Uuid7,
    pub community_id: Uuid7,
    pub data: Map<String, Value>,
}

impl RawEnvelope {
    pub fn parse(metadata: &[u8]) -> Result<Self, EnvelopeError> {
        let mut fields = strict_json::parse_object(metadata)?;
        if fields.len() != 4 {
            return Err(EnvelopeError::Fields);
        }
        let mut take = |name: &str| fields.remove(name).ok_or(EnvelopeError::Fields);

        let Value::String(type_name) = take("type")? else {
            return Err(EnvelopeError::Fields);
        };
        let request_id = uuid_field(&take("request_id")?, "request_id")?;
        let community_id = uuid_field(&take("community_id")?, "community_id")?;
        let Value::Object(data) = take("data")? else {
            return Err(EnvelopeError::Data);
        };
        Ok(Self {
            type_name,
            request_id,
            community_id,
            data,
        })
    }
}

/// The four-field metadata object shared by every network message.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub message_type: MessageType,
    pub request_id: Uuid7,
    pub community_id: Uuid7,
    pub data: Map<String, Value>,
}

impl Envelope {
    pub fn parse(metadata: &[u8]) -> Result<Self, EnvelopeError> {
        let raw = RawEnvelope::parse(metadata)?;
        let message_type = MessageType::from_wire(&raw.type_name)
            .ok_or(EnvelopeError::UnsupportedType(raw.type_name))?;
        Ok(Self {
            message_type,
            request_id: raw.request_id,
            community_id: raw.community_id,
            data: raw.data,
        })
    }
}

fn uuid_field(value: &Value, name: &'static str) -> Result<Uuid7, EnvelopeError> {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .ok_or(EnvelopeError::Uuid(name))
}
