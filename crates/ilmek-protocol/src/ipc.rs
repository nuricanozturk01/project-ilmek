//! Host ↔ backend messages on the loopback IPC connection (PROTOCOL.md §16).
//!
//! Same frame codec and envelope as the network protocol, but a disjoint set of `IPC_*`
//! message types that the network endpoints reject.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::frame::{Frame, FrameError};
use crate::ids::Uuid7;
use crate::message::{EnvelopeError, RawEnvelope};
use crate::strict_json::{self, JsonError, Profile};
use crate::wire::{
    AppResult, Cursor, ErrorData, Event, EventAck, EventAcked, Hash, Header, Method, Payload,
    Subscribed, SubscriptionRef, Token, Version,
};
use crate::wire::{parse_data, to_data};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcHello {
    pub token: Token,
    pub app_id: Uuid7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcReady {
    pub app_id: Uuid7,
}

/// `APP_CALL` data plus the principal the host authenticated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcCall {
    pub app_id: Uuid7,
    pub client_app_version: Version,
    pub method: Method,
    #[serde(deserialize_with = "present")]
    pub operation_id: Option<Uuid7>,
    pub payload_hash: Hash,
    pub principal_id: Uuid7,
}

/// `SUBSCRIBE` data plus the authenticated principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcSubscribe {
    pub app_id: Uuid7,
    pub client_app_version: Version,
    pub topic: Method,
    #[serde(deserialize_with = "present")]
    pub cursor: Option<Cursor>,
    pub principal_id: Uuid7,
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
    #[error("unknown IPC message type `{0}`")]
    UnknownType(String),
    #[error("{kind}: invalid data: {source}")]
    Data {
        kind: &'static str,
        source: serde_json::Error,
    },
    #[error("{0} must not carry a body")]
    UnexpectedBody(&'static str),
    #[error("{0} body is not a strict JSON object: {1}")]
    BodyJson(&'static str, JsonError),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("message could not be serialized")]
    Serialize(#[source] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcMessage {
    Hello(IpcHello),
    Ready(IpcReady),
    Call(Payload<IpcCall>),
    Result(Payload<AppResult>),
    Error(ErrorData),
    Subscribe(IpcSubscribe),
    Subscribed(Subscribed),
    Event(Payload<Event>),
    EventAck(EventAck),
    EventAcked(EventAcked),
    Unsubscribe(SubscriptionRef),
    Unsubscribed(SubscriptionRef),
}

fn data_error(kind: &'static str) -> impl FnOnce(crate::wire::WireError) -> IpcError {
    move |error| match error {
        crate::wire::WireError::Data { source, .. } => IpcError::Data { kind, source },
        crate::wire::WireError::Serialize(source) => IpcError::Serialize(source),
        _ => IpcError::UnexpectedBody(kind),
    }
}

fn bare<T: serde::de::DeserializeOwned>(
    kind: &'static str,
    data: serde_json::Map<String, Value>,
    body: &[u8],
) -> Result<T, IpcError> {
    if !body.is_empty() {
        return Err(IpcError::UnexpectedBody(kind));
    }
    parse_data(crate::MessageType::Ping, Value::Object(data)).map_err(data_error(kind))
}

fn rich<T: serde::de::DeserializeOwned>(
    kind: &'static str,
    data: serde_json::Map<String, Value>,
    body: Vec<u8>,
) -> Result<Payload<T>, IpcError> {
    strict_json::parse_object_with(&body, Profile::Body)
        .map_err(|e| IpcError::BodyJson(kind, e))?;
    let data =
        parse_data(crate::MessageType::Ping, Value::Object(data)).map_err(data_error(kind))?;
    Ok(Payload { data, body })
}

impl IpcMessage {
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Hello(_) => "IPC_HELLO",
            Self::Ready(_) => "IPC_READY",
            Self::Call(_) => "IPC_CALL",
            Self::Result(_) => "IPC_RESULT",
            Self::Error(_) => "IPC_ERROR",
            Self::Subscribe(_) => "IPC_SUBSCRIBE",
            Self::Subscribed(_) => "IPC_SUBSCRIBED",
            Self::Event(_) => "IPC_EVENT",
            Self::EventAck(_) => "IPC_EVENT_ACK",
            Self::EventAcked(_) => "IPC_EVENT_ACKED",
            Self::Unsubscribe(_) => "IPC_UNSUBSCRIBE",
            Self::Unsubscribed(_) => "IPC_UNSUBSCRIBED",
        }
    }

    pub fn decode(frame: &Frame) -> Result<(Header, Self), IpcError> {
        let raw = RawEnvelope::parse(&frame.metadata)?;
        let header = Header {
            request_id: raw.request_id,
            community_id: raw.community_id,
        };
        let (data, body) = (raw.data, frame.body.clone());
        let message = match raw.type_name.as_str() {
            "IPC_HELLO" => Self::Hello(bare("IPC_HELLO", data, &body)?),
            "IPC_READY" => Self::Ready(bare("IPC_READY", data, &body)?),
            "IPC_CALL" => Self::Call(rich("IPC_CALL", data, body)?),
            "IPC_RESULT" => Self::Result(rich("IPC_RESULT", data, body)?),
            "IPC_ERROR" => Self::Error(bare("IPC_ERROR", data, &body)?),
            "IPC_SUBSCRIBE" => Self::Subscribe(bare("IPC_SUBSCRIBE", data, &body)?),
            "IPC_SUBSCRIBED" => Self::Subscribed(bare("IPC_SUBSCRIBED", data, &body)?),
            "IPC_EVENT" => Self::Event(rich("IPC_EVENT", data, body)?),
            "IPC_EVENT_ACK" => Self::EventAck(bare("IPC_EVENT_ACK", data, &body)?),
            "IPC_EVENT_ACKED" => Self::EventAcked(bare("IPC_EVENT_ACKED", data, &body)?),
            "IPC_UNSUBSCRIBE" => Self::Unsubscribe(bare("IPC_UNSUBSCRIBE", data, &body)?),
            "IPC_UNSUBSCRIBED" => Self::Unsubscribed(bare("IPC_UNSUBSCRIBED", data, &body)?),
            other => return Err(IpcError::UnknownType(other.to_owned())),
        };
        Ok((header, message))
    }

    pub fn encode(&self, header: Header) -> Result<Frame, IpcError> {
        let (data, body): (Value, &[u8]) = match self {
            Self::Hello(d) => (to_data(d).map_err(data_error("IPC_HELLO"))?, &[]),
            Self::Ready(d) => (to_data(d).map_err(data_error("IPC_READY"))?, &[]),
            Self::Call(p) => (to_data(&p.data).map_err(data_error("IPC_CALL"))?, &p.body),
            Self::Result(p) => (to_data(&p.data).map_err(data_error("IPC_RESULT"))?, &p.body),
            Self::Error(d) => (to_data(d).map_err(data_error("IPC_ERROR"))?, &[]),
            Self::Subscribe(d) => (to_data(d).map_err(data_error("IPC_SUBSCRIBE"))?, &[]),
            Self::Subscribed(d) => (to_data(d).map_err(data_error("IPC_SUBSCRIBED"))?, &[]),
            Self::Event(p) => (to_data(&p.data).map_err(data_error("IPC_EVENT"))?, &p.body),
            Self::EventAck(d) => (to_data(d).map_err(data_error("IPC_EVENT_ACK"))?, &[]),
            Self::EventAcked(d) => (to_data(d).map_err(data_error("IPC_EVENT_ACKED"))?, &[]),
            Self::Unsubscribe(d) => (to_data(d).map_err(data_error("IPC_UNSUBSCRIBE"))?, &[]),
            Self::Unsubscribed(d) => (to_data(d).map_err(data_error("IPC_UNSUBSCRIBED"))?, &[]),
        };
        let mut envelope = serde_json::Map::with_capacity(4);
        envelope.insert("type".into(), Value::from(self.type_name()));
        envelope.insert(
            "request_id".into(),
            Value::from(header.request_id.to_string()),
        );
        envelope.insert(
            "community_id".into(),
            Value::from(header.community_id.to_string()),
        );
        envelope.insert("data".into(), data);
        let metadata = serde_json::to_vec(&envelope).map_err(IpcError::Serialize)?;
        Ok(Frame {
            metadata,
            body: body.to_vec(),
        })
    }
}
