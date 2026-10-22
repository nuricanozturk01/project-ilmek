use ilmek_client::Error as ClientError;
use ilmek_runtime::RuntimeError;
use serde::Serialize;

/// What the UI sees: a stable code it translates, never raw internals.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct UiError {
    pub(crate) code: &'static str,
}

impl UiError {
    pub(crate) const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl From<RuntimeError> for UiError {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::Address(_) => Self::new("address_invalid"),
            RuntimeError::Package(_) => Self::new("package_damaged"),
            RuntimeError::Invalid(_) => Self::new("invalid_input"),
            RuntimeError::Client(inner) => inner.into(),
            RuntimeError::Storage(_) | RuntimeError::Io(_) => Self::new("storage_failed"),
        }
    }
}

impl From<ClientError> for UiError {
    fn from(error: ClientError) -> Self {
        use ilmek_protocol::wire::ErrorCode;
        if let Some(code) = error.remote_code() {
            return Self::new(match code {
                ErrorCode::NotFound => "not_found",
                ErrorCode::AppUnavailable => "app_unavailable",
                ErrorCode::Unauthorized | ErrorCode::Forbidden => "forbidden",
                ErrorCode::TrustStale => "trust_stale",
                ErrorCode::RateLimited => "rate_limited",
                _ => "server_error",
            });
        }
        Self::new(match error {
            ClientError::Cancelled => "cancelled",
            ClientError::Quota { .. } => "quota",
            ClientError::Integrity(_) | ClientError::Package(_) => "integrity",
            ClientError::PublisherRevoked | ClientError::PeerRevoked => "revoked",
            ClientError::TrustStale => "trust_stale",
            ClientError::Tls(_) | ClientError::Session(_) => "unreachable",
            ClientError::Profile(_) => "profile_invalid",
            _ => "unknown",
        })
    }
}
