//! Owner-only files for private keys and tokens.
//!
//! On Unix files are created `0600` and directories `0700`. On other platforms the default
//! ACLs apply; that gap is documented in docs/SECURITY.md.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

/// Creates a new file readable only by the owner; refuses to overwrite.
pub fn write_secret(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Replaces a secret atomically: write a sibling, then rename over the target.
pub fn replace_secret(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staging = path.with_extension("new");
    match std::fs::remove_file(&staging) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    write_secret(&staging, bytes)?;
    std::fs::rename(&staging, path)
}

/// Creates a directory tree that only the owner may enter.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
