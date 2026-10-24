use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use super::data::{
    AppCall, AppResult, AppUnregistered, AppsListed, AssetChunk, AssetEnd, AssetInfo, Cancel,
    CancelAck, Drain, Empty, ErrorData, Event, EventAck, EventAcked, GetAsset, GetManifest,
    Goodbye, HeartbeatAck, Hello, HelloAck, HostHeartbeat, HostRegister, HostRegistered, ListApps,
    ManifestInfo, Nonce, RecordResult, RegisterApp, Resolve, Resolved, Subscribe, Subscribed,
    SubscriptionRef, UnregisterApp, UpdateApp,
};
use crate::frame::{Frame, FrameError};
use crate::ids::Uuid7;
use crate::message::{Envelope, EnvelopeError, MessageType};
use crate::strict_json::{self, JsonError, Profile};

#[derive(Debug, Error)]
pub enum WireError {
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
    #[error("{message_type:?}: invalid data: {source}")]
    Data {
        message_type: MessageType,
        source: serde_json::Error,
    },
    #[error("{0:?} must not carry a body")]
    UnexpectedBody(MessageType),
    #[error("{0:?} body is not a strict JSON object: {1}")]
    BodyJson(MessageType, JsonError),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("message could not be serialized")]
    Serialize(#[source] serde_json::Error),
}

/// Identifiers carried by every metadata object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub request_id: Uuid7,
    pub community_id: Uuid7,
}

/// A message whose frame body is meaningful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload<T> {
    pub data: T,
    pub body: Vec<u8>,
}

macro_rules! catalog {
    (
        bare { $($bare:ident($bare_ty:ty) = $bare_kind:ident,)* }
        body { $($rich:ident($rich_ty:ty) = $rich_kind:ident, json_body: $json:literal,)* }
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Message {
            $($bare($bare_ty),)*
            $($rich(Payload<$rich_ty>),)*
        }

        impl Message {
            #[must_use]
            pub fn message_type(&self) -> MessageType {
                match self {
                    $(Self::$bare(_) => MessageType::$bare_kind,)*
                    $(Self::$rich(_) => MessageType::$rich_kind,)*
                }
            }

            fn from_parts(kind: MessageType, data: Value, body: Vec<u8>) -> Result<Self, WireError> {
                match kind {
                    $(MessageType::$bare_kind => {
                        if !body.is_empty() {
                            return Err(WireError::UnexpectedBody(kind));
                        }
                        Ok(Self::$bare(parse_data(kind, data)?))
                    })*
                    $(MessageType::$rich_kind => {
                        if $json {
                            strict_json::parse_object_with(&body, Profile::Body)
                                .map_err(|e| WireError::BodyJson(kind, e))?;
                        }
                        Ok(Self::$rich(Payload { data: parse_data(kind, data)?, body }))
                    })*
                }
            }

            fn to_parts(&self) -> Result<(Value, &[u8]), WireError> {
                match self {
                    $(Self::$bare(data) => Ok((to_data(data)?, &[]) ),)*
                    $(Self::$rich(payload) => Ok((to_data(&payload.data)?, &payload.body)),)*
                }
            }
        }
    };
}

catalog! {
    bare {
        Hello(Hello) = Hello,
        HelloAck(HelloAck) = HelloAck,
        Ping(Nonce) = Ping,
        Pong(Nonce) = Pong,
        Goodbye(Goodbye) = Goodbye,
        GoodbyeAck(Empty) = GoodbyeAck,
        Drain(Drain) = Drain,
        Resolve(Resolve) = Resolve,
        Resolved(Resolved) = Resolved,
        ListApps(ListApps) = ListApps,
        HostRegister(HostRegister) = HostRegister,
        HostRegistered(HostRegistered) = HostRegistered,
        HostHeartbeat(HostHeartbeat) = HostHeartbeat,
        HeartbeatAck(HeartbeatAck) = HeartbeatAck,
        RegisterApp(RegisterApp) = RegisterApp,
        AppRegistered(RecordResult) = AppRegistered,
        UnregisterApp(UnregisterApp) = UnregisterApp,
        AppUnregistered(AppUnregistered) = AppUnregistered,
        UpdateApp(UpdateApp) = UpdateApp,
        AppUpdated(RecordResult) = AppUpdated,
        GetManifest(GetManifest) = GetManifest,
        Subscribe(Subscribe) = Subscribe,
        Subscribed(Subscribed) = Subscribed,
        EventAck(EventAck) = EventAck,
        EventAcked(EventAcked) = EventAcked,
        Unsubscribe(SubscriptionRef) = Unsubscribe,
        Unsubscribed(SubscriptionRef) = Unsubscribed,
        GetAsset(GetAsset) = GetAsset,
        AssetInfo(AssetInfo) = AssetInfo,
        AssetEnd(AssetEnd) = AssetEnd,
        Cancel(Cancel) = Cancel,
        CancelAck(CancelAck) = CancelAck,
        Error(ErrorData) = Error,
    }
    body {
        AppsListed(AppsListed) = AppsListed, json_body: true,
        Manifest(ManifestInfo) = Manifest, json_body: false,
        AppCall(AppCall) = AppCall, json_body: true,
        AppResult(AppResult) = AppResult, json_body: true,
        Event(Event) = Event, json_body: true,
        AssetChunk(AssetChunk) = AssetChunk, json_body: false,
    }
}

impl Message {
    /// Decodes one frame into its typed message.
    pub fn decode(frame: &Frame) -> Result<(Header, Self), WireError> {
        let envelope = Envelope::parse(&frame.metadata)?;
        let header = Header {
            request_id: envelope.request_id,
            community_id: envelope.community_id,
        };
        let message = Self::from_parts(
            envelope.message_type,
            Value::Object(envelope.data),
            frame.body.clone(),
        )?;
        Ok((header, message))
    }

    /// Encodes the message with the shared four-field envelope.
    pub fn encode(&self, header: Header) -> Result<Frame, WireError> {
        let (data, body) = self.to_parts()?;
        let mut envelope = serde_json::Map::with_capacity(4);
        envelope.insert("type".into(), Value::from(self.message_type().as_str()));
        envelope.insert(
            "request_id".into(),
            Value::from(header.request_id.to_string()),
        );
        envelope.insert(
            "community_id".into(),
            Value::from(header.community_id.to_string()),
        );
        envelope.insert("data".into(), data);
        let metadata = serde_json::to_vec(&envelope).map_err(WireError::Serialize)?;
        Ok(Frame {
            metadata,
            body: body.to_vec(),
        })
    }
}

pub(crate) fn parse_data<T: DeserializeOwned>(
    kind: MessageType,
    data: Value,
) -> Result<T, WireError> {
    serde_json::from_value(data).map_err(|source| WireError::Data {
        message_type: kind,
        source,
    })
}

pub(crate) fn to_data<T: Serialize>(data: &T) -> Result<Value, WireError> {
    serde_json::to_value(data).map_err(WireError::Serialize)
}
