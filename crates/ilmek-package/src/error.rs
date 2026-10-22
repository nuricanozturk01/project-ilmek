use ilmek_identity::PublisherError;
use ilmek_protocol::manifest::ManifestError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("i/o error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("manifest hash differs from the expected {expected}")]
    ManifestHash { expected: String },
    #[error("{0}")]
    Signature(#[from] PublisherError),
    #[error("signature file is not valid base64url")]
    SignatureEncoding,
    #[error("manifest names publisher key {manifest}, but {actual} was used to verify")]
    KeyMismatch { manifest: String, actual: String },
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("`{0}` is not a regular file or directory (links and special files are refused)")]
    NotRegular(String),
    #[error("file `{0}` is on disk but not in the manifest")]
    Unlisted(String),
    #[error("file `{0}` is in the manifest but missing on disk")]
    Missing(String),
    #[error("file `{path}` has size {actual}, manifest says {declared}")]
    SizeMismatch {
        path: String,
        declared: u64,
        actual: u64,
    },
    #[error("file `{0}` does not match its SHA-256")]
    HashMismatch(String),
    #[error("invalid package source: {0}")]
    Source(String),
    #[error("destination already exists: {0}")]
    DestinationExists(String),
}

impl PackageError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}
