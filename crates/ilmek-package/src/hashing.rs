use std::fs::File;
use std::io::Read;
use std::path::Path;

use ilmek_protocol::wire::Hash;
use sha2::{Digest, Sha256};

use crate::error::PackageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileDigest {
    pub size: u64,
    pub sha256: Hash,
}

/// Streams a file through SHA-256 without loading it into memory.
pub fn hash_file(path: &Path) -> Result<FileDigest, PackageError> {
    let mut file = File::open(path).map_err(|e| PackageError::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| PackageError::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok(FileDigest {
        size,
        sha256: Hash::from_bytes(hasher.finalize().into()),
    })
}
