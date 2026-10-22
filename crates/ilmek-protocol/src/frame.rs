//! Byte-level framing (PROTOCOL.md §5).

use thiserror::Error;

pub const MAGIC: [u8; 4] = *b"ILM1";
pub const PREFIX_LEN: usize = 12;
pub const MAX_METADATA_LEN: usize = 65_536;
pub const MAX_BODY_LEN: usize = 262_144;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FrameError {
    #[error("bad frame magic")]
    BadMagic,
    #[error("metadata length {0} outside 1..={MAX_METADATA_LEN}")]
    MetadataLength(usize),
    #[error("body length {0} exceeds {MAX_BODY_LEN}")]
    BodyLength(usize),
    #[error("stream ended inside a frame")]
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub metadata: Vec<u8>,
    pub body: Vec<u8>,
}

impl Frame {
    /// Serializes the frame, rejecting sizes the receiver would refuse.
    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        let (metadata_len, body_len) = check_lengths(self.metadata.len(), self.body.len())?;
        let mut out = Vec::with_capacity(PREFIX_LEN + self.metadata.len() + self.body.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&metadata_len.to_be_bytes());
        out.extend_from_slice(&body_len.to_be_bytes());
        out.extend_from_slice(&self.metadata);
        out.extend_from_slice(&self.body);
        Ok(out)
    }
}

/// Incremental decoder: TCP reads are not frame boundaries.
///
/// Buffered memory is bounded by bytes actually received; declared lengths are validated
/// before any payload is awaited.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Returns the next complete frame, or `None` when more bytes are needed.
    ///
    /// After an error the stream is unusable and the connection must be closed.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        let magic_seen = self.buf.len().min(MAGIC.len());
        if self.buf[..magic_seen] != MAGIC[..magic_seen] {
            return Err(FrameError::BadMagic);
        }
        if self.buf.len() < PREFIX_LEN {
            return Ok(None);
        }
        let metadata_len = read_length(&self.buf[4..8]);
        let body_len = read_length(&self.buf[8..12]);
        check_lengths(metadata_len, body_len)?;

        let total = PREFIX_LEN + metadata_len + body_len;
        if self.buf.len() < total {
            return Ok(None);
        }
        let metadata_end = PREFIX_LEN + metadata_len;
        let frame = Frame {
            metadata: self.buf[PREFIX_LEN..metadata_end].to_vec(),
            body: self.buf[metadata_end..total].to_vec(),
        };
        self.buf.drain(..total);
        Ok(Some(frame))
    }

    /// Number of received bytes not yet consumed by a complete frame.
    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.buf.len()
    }

    /// Call on EOF: leftover bytes mean the peer cut a frame short.
    pub fn finish(&self) -> Result<(), FrameError> {
        if self.buf.is_empty() {
            Ok(())
        } else {
            Err(FrameError::Truncated)
        }
    }
}

fn check_lengths(metadata: usize, body: usize) -> Result<(u32, u32), FrameError> {
    let metadata_len = u32::try_from(metadata)
        .ok()
        .filter(|_| (1..=MAX_METADATA_LEN).contains(&metadata))
        .ok_or(FrameError::MetadataLength(metadata))?;
    let body_len = u32::try_from(body)
        .ok()
        .filter(|_| body <= MAX_BODY_LEN)
        .ok_or(FrameError::BodyLength(body))?;
    Ok((metadata_len, body_len))
}

fn read_length(bytes: &[u8]) -> usize {
    let mut field = [0u8; 4];
    field.copy_from_slice(bytes);
    u32::from_be_bytes(field) as usize
}
