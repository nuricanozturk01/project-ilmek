use ilmek_core::session::ClientError;
use ilmek_core::tls::TlsError;
use ilmek_core::trust::TrustError;
use ilmek_identity::{PkiError, ProfileError};
use ilmek_protocol::wire::{ErrorCode, ErrorData};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Session(#[from] ClientError),
    #[error(transparent)]
    Tls(#[from] TlsError),
    #[error(transparent)]
    Pki(#[from] PkiError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
    #[error(transparent)]
    Trust(#[from] TrustError),
    #[error("storage: {0}")]
    Storage(#[from] std::io::Error),
    #[error("stored device data is damaged: {0}")]
    Corrupt(&'static str),
    #[error("trust information is older than 24 hours or missing; refresh it from the registry")]
    TrustStale,
    #[error("the peer's certificate has been revoked")]
    PeerRevoked,
    #[error("unexpected reply: {0}")]
    UnexpectedReply(&'static str),
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    #[error(transparent)]
    Package(#[from] ilmek_package::PackageError),
    #[error("the downloaded content does not match its hash: {0}")]
    Integrity(String),
    #[error("not enough room in the package cache ({needed} bytes needed, quota {quota})")]
    Quota { needed: u64, quota: u64 },
    #[error("the download was cancelled")]
    Cancelled,
    #[error("the publisher key in the registry record is not usable: {0}")]
    PublisherKey(String),
    #[error("the publisher key was revoked")]
    PublisherRevoked,
}

impl Error {
    /// The server's error code, when the failure was an answer from the server.
    #[must_use]
    pub fn remote_code(&self) -> Option<ErrorCode> {
        self.remote().map(|data| data.code)
    }

    #[must_use]
    pub fn remote(&self) -> Option<&ErrorData> {
        match self {
            Self::Session(ClientError::Remote(data)) => Some(data),
            _ => None,
        }
    }

    /// `true` when the outcome of a request is unknown (timeout or connection loss).
    #[must_use]
    pub fn outcome_unknown(&self) -> bool {
        matches!(
            self,
            Self::Session(ClientError::Timeout | ClientError::Closed | ClientError::Transport(_))
                | Self::Tls(_)
        )
    }
}
