//! Connection sessions: the HELLO state machine and request routing (PROTOCOL.md §6, §7).

mod client;
mod modes;
mod server;

pub use client::{ClientError, ClientParams, ClientSession, ResponseStream};
pub use server::{
    ConnectionParams, Handler, Reply, ReplyError, ServerSessionConfig, SessionEnd, SessionInfo,
    serve_connection,
};

use ilmek_protocol::wire::{ErrorCode, ErrorData, Header, Message};
use ilmek_protocol::{MessageType, Uuid7};

use crate::transport::Priority;

fn priority(kind: MessageType) -> Priority {
    match kind {
        MessageType::Ping
        | MessageType::Pong
        | MessageType::Cancel
        | MessageType::CancelAck
        | MessageType::EventAck
        | MessageType::EventAcked
        | MessageType::Unsubscribe
        | MessageType::Unsubscribed
        | MessageType::Goodbye
        | MessageType::GoodbyeAck => Priority::Control,
        _ => Priority::Normal,
    }
}

fn error_message(code: ErrorCode, text: &str, retryable: bool) -> Message {
    Message::Error(ErrorData {
        code,
        message: text.parse().unwrap_or_default(),
        retryable,
        retry_after_ms: None,
        details: serde_json::Map::new(),
    })
}

fn header(request_id: Uuid7, community_id: Uuid7) -> Header {
    Header {
        request_id,
        community_id,
    }
}
