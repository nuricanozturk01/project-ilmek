//! Typed ILMEK/1 messages (PROTOCOL.md §6-§13).

mod codec;
pub(crate) use codec::{parse_data, to_data};
mod data;
mod types;

pub use codec::{Header, Message, Payload, WireError};
pub use data::*;
pub use types::*;
