//! SDK for application backends.
//!
//! A backend is a trusted local process started by `ilmek-host`. It receives requests from the
//! host over loopback IPC (PROTOCOL.md §16), never from the network, and gets the caller's
//! authenticated principal from the host.

mod backend;
mod config;
mod dedupe;
mod error;
mod events;
mod runtime;

pub use backend::{Backend, CallContext, MethodKind, SubscribeContext};
pub use config::BackendConfig;
pub use dedupe::{Operations, WriteOutcome};
pub use error::{AppError, SdkError};
pub use events::Events;
pub use runtime::{Runtime, Tuning, run, run_tuned, run_with_config};
