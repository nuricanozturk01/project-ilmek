use ilmek_package::PackageError;
use ilmek_store::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("no such application")]
    UnknownApp,
    #[error("no such version")]
    UnknownVersion,
    #[error("{0}")]
    Conflict(&'static str),
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("i/o error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("stored data is damaged: {0}")]
    Corrupt(&'static str),
    #[error("device identity: {0}")]
    Device(String),
}

impl HostError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

impl From<ilmek_store::rusqlite::Error> for HostError {
    fn from(error: ilmek_store::rusqlite::Error) -> Self {
        Self::Store(StoreError::Sqlite(error))
    }
}
