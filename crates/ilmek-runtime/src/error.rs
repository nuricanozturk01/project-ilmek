use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Client(#[from] ilmek_client::Error),
    #[error("storage: {0}")]
    Storage(#[from] ilmek_store::StoreError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid address: {0}")]
    Address(#[from] ilmek_protocol::address::AddressError),
    #[error("the package is damaged: {0}")]
    Package(String),
    #[error("{0}")]
    Invalid(&'static str),
}
