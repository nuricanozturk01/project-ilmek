//! Frame IO over any byte stream: bounded reads, one prioritized writer per connection
//! (PROTOCOL.md §5, §7, §19).

use std::sync::Arc;
use std::time::Duration;

use ilmek_protocol::{Frame, FrameDecoder, FrameError};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};

/// A started frame must complete within this time; progress does not extend it.
pub const FRAME_DEADLINE: Duration = Duration::from_secs(30);
/// A frame that cannot be written for this long means the peer stopped reading.
pub const WRITE_DEADLINE: Duration = Duration::from_secs(30);
const WRITER_BYTE_BUDGET: usize = 2 * 1024 * 1024;
const NORMAL_QUEUE_FRAMES: usize = 64;
const CONTROL_QUEUE_FRAMES: usize = 16;
const READ_CHUNK: usize = 16 * 1024;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame not completed within {FRAME_DEADLINE:?}")]
    FrameTimeout,
    #[error("peer did not accept data within {WRITE_DEADLINE:?}")]
    WriteTimeout,
    #[error("connection closed")]
    Closed,
}

#[derive(Debug)]
pub struct FrameReader<R> {
    reader: R,
    decoder: FrameDecoder,
    partial_since: Option<Instant>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: FrameDecoder::new(),
            partial_since: None,
        }
    }

    /// Next frame, or `None` on a clean end of stream. Waiting for a frame to *start* has no
    /// deadline here; callers apply their own idle policy.
    pub async fn next(&mut self) -> Result<Option<Frame>, TransportError> {
        let mut chunk = vec![0u8; READ_CHUNK];
        loop {
            if let Some(frame) = self.decoder.next_frame()? {
                self.partial_since = (self.decoder.buffered_len() > 0).then(Instant::now);
                return Ok(Some(frame));
            }
            let read = match self.partial_since {
                Some(started) => {
                    let remaining = FRAME_DEADLINE.saturating_sub(started.elapsed());
                    timeout(remaining, self.reader.read(&mut chunk))
                        .await
                        .map_err(|_| TransportError::FrameTimeout)??
                }
                None => self.reader.read(&mut chunk).await?,
            };
            if read == 0 {
                self.decoder.finish()?;
                return Ok(None);
            }
            self.partial_since.get_or_insert_with(Instant::now);
            self.decoder.push(&chunk[..read]);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// PING/PONG, CANCEL, EVENT_ACK, UNSUBSCRIBE, GOODBYE: never stuck behind bulk replies.
    Control,
    Normal,
}

struct Queued {
    bytes: Vec<u8>,
    _budget: OwnedSemaphorePermit,
}

/// Cloneable sender into the connection's single writer task.
#[derive(Debug, Clone)]
pub struct FrameSender {
    control: mpsc::Sender<Queued>,
    normal: mpsc::Sender<Queued>,
    budget: Arc<Semaphore>,
}

impl FrameSender {
    /// Queues a frame; waits when the queue or the 2 MiB byte budget is full.
    pub async fn send(&self, frame: &Frame, priority: Priority) -> Result<(), TransportError> {
        let bytes = frame.encode()?;
        let permits = u32::try_from(bytes.len()).map_err(|_| TransportError::Closed)?;
        let budget = Arc::clone(&self.budget)
            .acquire_many_owned(permits)
            .await
            .map_err(|_| TransportError::Closed)?;
        let queue = match priority {
            Priority::Control => &self.control,
            Priority::Normal => &self.normal,
        };
        queue
            .send(Queued {
                bytes,
                _budget: budget,
            })
            .await
            .map_err(|_| TransportError::Closed)
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.normal.is_closed()
    }
}

/// Starts the writer task. It ends when every sender is dropped (after flushing) or on a
/// write failure, which closes the senders.
pub fn spawn_writer<W>(writer: W) -> (FrameSender, JoinHandle<Result<(), TransportError>>)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (control_tx, control_rx) = mpsc::channel(CONTROL_QUEUE_FRAMES);
    let (normal_tx, normal_rx) = mpsc::channel(NORMAL_QUEUE_FRAMES);
    let sender = FrameSender {
        control: control_tx,
        normal: normal_tx,
        budget: Arc::new(Semaphore::new(WRITER_BYTE_BUDGET)),
    };
    (
        sender,
        tokio::spawn(write_loop(writer, control_rx, normal_rx)),
    )
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut control: mpsc::Receiver<Queued>,
    mut normal: mpsc::Receiver<Queued>,
) -> Result<(), TransportError> {
    loop {
        let queued = tokio::select! {
            biased;
            Some(queued) = control.recv() => queued,
            Some(queued) = normal.recv() => queued,
            else => break,
        };
        timeout(WRITE_DEADLINE, async {
            writer.write_all(&queued.bytes).await?;
            writer.flush().await
        })
        .await
        .map_err(|_| TransportError::WriteTimeout)??;
    }
    writer.shutdown().await?;
    Ok(())
}
