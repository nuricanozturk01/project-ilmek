use ilmek_protocol::wire::ErrorCode;
use thiserror::Error;

/// A failure a backend method reports to the caller.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code:?}: {message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidMessage, message)
    }

    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    #[must_use]
    pub fn revision_conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::RevisionConflict, message)
    }

    /// Storage failures are retryable but never leak internals.
    #[must_use]
    pub fn storage(source: &dyn std::fmt::Display) -> Self {
        tracing::error!(error = %source, "backend storage failure");
        Self {
            code: ErrorCode::Internal,
            message: "storage error".into(),
            retryable: true,
        }
    }
}

impl From<ilmek_store::StoreError> for AppError {
    fn from(error: ilmek_store::StoreError) -> Self {
        Self::storage(&error)
    }
}

impl From<ilmek_store::rusqlite::Error> for AppError {
    fn from(error: ilmek_store::rusqlite::Error) -> Self {
        Self::storage(&error)
    }
}

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("start-up configuration is missing or invalid: {0}")]
    Config(String),
    #[error("cannot reach the host: {0}")]
    Connect(#[from] std::io::Error),
    #[error("host handshake failed: {0}")]
    Handshake(&'static str),
    #[error("database: {0}")]
    Store(#[from] ilmek_store::StoreError),
}
