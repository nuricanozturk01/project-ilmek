//! ILMEK/1 wire protocol primitives. No I/O lives here.

pub mod digest;
pub mod frame;
pub mod ids;
pub mod ipc;
pub mod manifest;
pub mod message;
pub mod strict_json;
pub mod wire;

pub use frame::{Frame, FrameDecoder, FrameError};
pub use ids::Uuid7;
pub use message::{Envelope, EnvelopeError, MessageType, RawEnvelope};
pub use strict_json::JsonError;
