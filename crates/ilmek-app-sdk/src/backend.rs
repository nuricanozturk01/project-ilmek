use std::future::Future;

use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Cursor, Hash, Method, Version};

use crate::error::AppError;

/// Whether a method changes state. Decided by the backend, never by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// Must be called without an operation id.
    Read,
    /// Must carry an operation id; callers retry it with the same id.
    Write,
}

#[derive(Debug, Clone)]
pub struct CallContext {
    pub app_id: Uuid7,
    pub method: Method,
    /// Authenticated by the host from the TLS certificate.
    pub principal_id: Uuid7,
    pub client_app_version: Version,
    pub operation_id: Option<Uuid7>,
    /// Host-verified hash of the exact request body, for dedupe.
    pub payload_hash: Hash,
}

#[derive(Debug, Clone)]
pub struct SubscribeContext {
    pub app_id: Uuid7,
    pub topic: Method,
    pub principal_id: Uuid7,
    pub client_app_version: Version,
    pub cursor: Option<Cursor>,
}

pub trait Backend: Send + Sync + 'static {
    /// The complete method table. Unlisted methods are answered with `NOT_FOUND`.
    fn methods(&self) -> &'static [(&'static str, MethodKind)];

    /// Handles one call; `body` is the exact JSON object the client sent.
    fn call(
        &self,
        context: CallContext,
        body: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, AppError>> + Send;

    /// Decides whether `context.principal_id` may follow `context.topic`.
    fn authorize_subscribe(
        &self,
        context: &SubscribeContext,
    ) -> impl Future<Output = Result<(), AppError>> + Send {
        let _ = context;
        async { Err(AppError::forbidden("this application has no event topics")) }
    }
}
