use ilmek_protocol::wire::ErrorCode;
use ilmek_store::StoreError;
use thiserror::Error;

/// Failure of a registry operation, mapped to a wire error code at the API edge.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(&'static str),
    #[error("not found")]
    NotFound,
    #[error("name conflict: {0}")]
    NameConflict(&'static str),
    #[error("revision conflict")]
    RevisionConflict,
    #[error("version conflict: {0}")]
    VersionConflict(&'static str),
    #[error("operation id reused with different content")]
    OperationConflict,
    #[error("invalid request: {0}")]
    Invalid(&'static str),
    #[error("signature invalid: {0}")]
    SignatureInvalid(&'static str),
    #[error("cursor expired")]
    CursorExpired,
    #[error("rate limited; retry in {retry_after_ms} ms")]
    RateLimited { retry_after_ms: u64 },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

impl From<ilmek_store::rusqlite::Error> for RegistryError {
    fn from(error: ilmek_store::rusqlite::Error) -> Self {
        Self::Store(StoreError::Sqlite(error))
    }
}

impl RegistryError {
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Unauthorized => ErrorCode::Unauthorized,
            Self::Forbidden(_) => ErrorCode::Forbidden,
            Self::NotFound => ErrorCode::NotFound,
            Self::NameConflict(_) => ErrorCode::NameConflict,
            Self::RevisionConflict => ErrorCode::RevisionConflict,
            Self::VersionConflict(_) => ErrorCode::VersionConflict,
            Self::OperationConflict => ErrorCode::OperationConflict,
            Self::Invalid(_) => ErrorCode::InvalidMessage,
            Self::SignatureInvalid(_) => ErrorCode::SignatureInvalid,
            Self::CursorExpired => ErrorCode::CursorExpired,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Store(_) | Self::Internal(_) => ErrorCode::Internal,
        }
    }

    /// Text that is safe to send to the peer; internals are never exposed.
    #[must_use]
    pub fn public_message(&self) -> String {
        match self {
            Self::Store(_) | Self::Internal(_) => "internal error".to_owned(),
            other => other.to_string(),
        }
    }

    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms } => Some(*retry_after_ms),
            _ => None,
        }
    }

    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Store(_))
    }
}
