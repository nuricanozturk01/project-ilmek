//! Plain TCP connect with a deadline. There is no TLS in this build.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use thiserror::Error;
use tokio::net::TcpStream;
use tokio::time::timeout;

const CONNECT_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("tcp connect failed: {0}")]
    Connect(#[source] io::Error),
    #[error("tcp connect timed out")]
    Timeout,
}

/// Opens a TCP connection to `address`.
pub async fn connect(address: SocketAddr) -> Result<TcpStream, ConnectError> {
    let stream = timeout(CONNECT_DEADLINE, TcpStream::connect(address))
        .await
        .map_err(|_| ConnectError::Timeout)?
        .map_err(ConnectError::Connect)?;
    // Small request/response frames; do not wait for Nagle.
    let _ = stream.set_nodelay(true);
    Ok(stream)
}
