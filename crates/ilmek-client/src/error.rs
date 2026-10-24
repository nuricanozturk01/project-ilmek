use ilmek_core::community::ProfileError;
use ilmek_core::discovery::DiscoveryError;
use ilmek_core::net::ConnectError;
use ilmek_core::session::ClientError;
use ilmek_protocol::wire::{ErrorCode, ErrorData};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Session(#[from] ClientError),
    #[error(transparent)]
    Connect(#[from] ConnectError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("storage: {0}")]
    Storage(#[from] std::io::Error),
    #[error("stored device data is damaged: {0}")]
    Corrupt(&'static str),
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
                | Self::Connect(_)
        )
    }
}
