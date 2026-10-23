//! Consistent backups of a data directory (REQUIREMENTS §22).
//!
//! SQLite files are copied with the online backup API, so a running, WAL-mode database yields a
//! consistent copy. Everything else is copied byte for byte. Every file is listed with its
//! SHA-256 in a manifest that [`verify_backup`] and [`restore_tree`] check before trusting
//! anything. Private keys (`*.key.der`) are left out unless the operator asks for them, and logs,
//! earlier backups and staging areas are never included.

use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::{StoreError, backup_file, restore, verify_integrity};

/// Name of the manifest inside a backup directory.
pub const MANIFEST_FILE: &str = "backup-manifest.json";

const FORMAT: u32 = 1;
const SKIPPED_DIRECTORIES: [&str; 3] = ["logs", "backups", "staging"];
const SKIPPED_SUFFIXES: [&str; 4] = ["-wal", "-shm", "-journal", ".ilmek-new"];
const KEY_SUFFIX: &str = ".key.der";

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("{what}: {source}")]
    Io {
        what: String,
        source: std::io::Error,
    },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("the backup manifest is unusable: {0}")]
    Manifest(String),
    #[error("backup file {path} does not match the manifest: {reason}")]
    Mismatch { path: String, reason: String },
    #[error("{0} already exists; refusing to overwrite it")]
    DestinationExists(PathBuf),
    #[error("{0} is not an empty directory; restore only into a new or empty directory")]
    DestinationNotEmpty(PathBuf),
    #[error("{0} is a symbolic link or special file, which backups do not follow")]
    Unsupported(PathBuf),
}

impl BackupError {
    fn io(what: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            what: what.into(),
            source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Sqlite,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    path: String,
    kind: Kind,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    created_unix: u64,
    keys_included: bool,
    /// Keys that exist in the source but were deliberately not copied.
    withheld_keys: Vec<String>,
    entries: Vec<Entry>,
}

/// What a backup contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    pub files: usize,
    pub databases: usize,
    pub bytes: u64,
    pub keys_included: bool,
    /// Relative paths of private keys that were left out. Without them a restore is incomplete.
    pub withheld_keys: Vec<String>,
}

/// What a restore put back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub files: usize,
    pub databases: usize,
    pub keys_included: bool,
    pub withheld_keys: Vec<String>,
}

/// Copies `source` into the new directory `destination`.
///
/// The backup is assembled next to the destination and renamed into place, so an interrupted
/// run never leaves something that looks like a finished backup.
pub fn backup_tree(
    source: &Path,
    destination: &Path,
    include_keys: bool,
) -> Result<BackupReport, BackupError> {
    if destination.exists() {
        return Err(BackupError::DestinationExists(destination.to_path_buf()));
    }
    let staging = sibling(destination, ".partial");
    if staging.exists() {
        return Err(BackupError::DestinationExists(staging));
    }
    create_private_dir(&staging)?;
    let result = write_backup(source, &staging, include_keys);
    match result {
        Ok(report) => {
            fs::rename(&staging, destination)
                .map_err(|e| BackupError::io("finish the backup", e))?;
            Ok(report)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn write_backup(
    source: &Path,
    staging: &Path,
    include_keys: bool,
) -> Result<BackupReport, BackupError> {
    let mut sources = Vec::new();
    collect(source, Path::new(""), true, &mut sources)?;
    sources.sort();

    let mut entries = Vec::new();
    let mut withheld_keys = Vec::new();
    for relative in sources {
        let name = portable(&relative);
        if is_key(&relative) && !include_keys {
            withheld_keys.push(name);
            continue;
        }
        let from = source.join(&relative);
        let to = staging.join(&relative);
        if let Some(parent) = to.parent() {
            create_private_dirs(staging, parent)?;
        }
        let kind = if relative.extension().is_some_and(|e| e == "db") {
            backup_file(&from, &to)?;
            Kind::Sqlite
        } else {
            fs::copy(&from, &to)
                .map_err(|e| BackupError::io(format!("copy {}", from.display()), e))?;
            Kind::File
        };
        make_private(&to)?;
        let (sha256, size) = digest(&to)?;
        entries.push(Entry {
            path: name,
            kind,
            size,
            sha256,
        });
    }

    let manifest = Manifest {
        format: FORMAT,
        created_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        keys_included: include_keys,
        withheld_keys: withheld_keys.clone(),
        entries,
    };
    let text =
        serde_json::to_vec_pretty(&manifest).map_err(|e| BackupError::Manifest(e.to_string()))?;
    let manifest_path = staging.join(MANIFEST_FILE);
    fs::write(&manifest_path, text).map_err(|e| BackupError::io("write the manifest", e))?;
    make_private(&manifest_path)?;

    Ok(BackupReport {
        files: manifest.entries.len(),
        databases: manifest
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Sqlite)
            .count(),
        bytes: manifest.entries.iter().map(|e| e.size).sum(),
        keys_included: include_keys,
        withheld_keys,
    })
}

/// Checks a backup directory without changing anything: manifest, listed files, sizes, hashes,
/// SQLite integrity, and that nothing unlisted is present.
pub fn verify_backup(backup: &Path) -> Result<BackupReport, BackupError> {
    let manifest = read_manifest(backup)?;
    for entry in &manifest.entries {
        let path = checked_path(&entry.path)?;
        let stored = backup.join(&path);
        let mismatch = |reason: &str| BackupError::Mismatch {
            path: entry.path.clone(),
            reason: reason.to_owned(),
        };
        let metadata = fs::symlink_metadata(&stored).map_err(|_| mismatch("file is missing"))?;
        if !metadata.is_file() {
            return Err(mismatch("not a regular file"));
        }
        let (sha256, size) = digest(&stored)?;
        if size != entry.size {
            return Err(mismatch("size differs"));
        }
        if sha256 != entry.sha256 {
            return Err(mismatch("contents differ (hash)"));
        }
        if entry.kind == Kind::Sqlite {
            let connection = Connection::open_with_flags(&stored, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(StoreError::from)?;
            verify_integrity(&connection)?;
        }
    }

    let mut present = Vec::new();
    collect(backup, Path::new(""), false, &mut present)?;
    for relative in present {
        let name = portable(&relative);
        if name != MANIFEST_FILE && !manifest.entries.iter().any(|e| e.path == name) {
            return Err(BackupError::Mismatch {
                path: name,
                reason: "file is not listed in the manifest".to_owned(),
            });
        }
    }

    Ok(BackupReport {
        files: manifest.entries.len(),
        databases: manifest
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Sqlite)
            .count(),
        bytes: manifest.entries.iter().map(|e| e.size).sum(),
        keys_included: manifest.keys_included,
        withheld_keys: manifest.withheld_keys,
    })
}

/// Restores a verified backup into `destination`, which must not exist or must be empty.
///
/// The data is assembled next to the destination and renamed into place; a failure leaves the
/// destination as it was.
pub fn restore_tree(backup: &Path, destination: &Path) -> Result<RestoreReport, BackupError> {
    let verified = verify_backup(backup)?;
    let manifest = read_manifest(backup)?;

    if destination.exists() {
        let mut contents = fs::read_dir(destination)
            .map_err(|_| BackupError::DestinationNotEmpty(destination.to_path_buf()))?;
        if contents.next().is_some() {
            return Err(BackupError::DestinationNotEmpty(destination.to_path_buf()));
        }
    }
    if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| BackupError::io(format!("create {}", parent.display()), e))?;
    }
    let staging = sibling(destination, ".restoring");
    if staging.exists() {
        return Err(BackupError::DestinationExists(staging));
    }
    create_private_dir(&staging)?;

    let result = restore_entries(&manifest, backup, &staging).and_then(|()| {
        if destination.exists() {
            fs::remove_dir(destination)
                .map_err(|e| BackupError::io("replace the empty destination", e))?;
        }
        fs::rename(&staging, destination).map_err(|e| BackupError::io("finish the restore", e))
    });
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(RestoreReport {
        files: verified.files,
        databases: verified.databases,
        keys_included: verified.keys_included,
        withheld_keys: verified.withheld_keys,
    })
}

fn restore_entries(manifest: &Manifest, backup: &Path, staging: &Path) -> Result<(), BackupError> {
    for entry in &manifest.entries {
        let relative = checked_path(&entry.path)?;
        let from = backup.join(&relative);
        let to = staging.join(&relative);
        if let Some(parent) = to.parent() {
            create_private_dirs(staging, parent)?;
        }
        match entry.kind {
            Kind::Sqlite => restore(&from, &to)?,
            Kind::File => {
                fs::copy(&from, &to)
                    .map_err(|e| BackupError::io(format!("restore {}", entry.path), e))?;
            }
        }
        make_private(&to)?;
    }
    Ok(())
}

fn read_manifest(backup: &Path) -> Result<Manifest, BackupError> {
    let bytes = fs::read(backup.join(MANIFEST_FILE))
        .map_err(|e| BackupError::io("read the backup manifest", e))?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|e| BackupError::Manifest(e.to_string()))?;
    if manifest.format != FORMAT {
        return Err(BackupError::Manifest(format!(
            "format {} is not supported (this build reads format {FORMAT})",
            manifest.format
        )));
    }
    Ok(manifest)
}

/// Relative, forward-looking paths only: no root, no `..`, no empty names.
fn checked_path(text: &str) -> Result<PathBuf, BackupError> {
    let path = Path::new(text);
    let safe = !text.is_empty()
        && !text.contains('\\')
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if safe {
        Ok(path.to_path_buf())
    } else {
        Err(BackupError::Manifest(format!("unsafe path `{text}`")))
    }
}

/// Lists the regular files below `root/relative`. With `skip_excluded`, leaves out what backups
/// never contain (logs, earlier backups, staging areas, SQLite side files).
fn collect(
    root: &Path,
    relative: &Path,
    skip_excluded: bool,
    found: &mut Vec<PathBuf>,
) -> Result<(), BackupError> {
    let directory = root.join(relative);
    let entries = fs::read_dir(&directory)
        .map_err(|e| BackupError::io(format!("read {}", directory.display()), e))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| BackupError::io(format!("read {}", directory.display()), e))?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        let child = relative.join(&name);
        let kind = entry
            .file_type()
            .map_err(|e| BackupError::io(format!("inspect {}", child.display()), e))?;
        if kind.is_dir() {
            if !(skip_excluded && SKIPPED_DIRECTORIES.contains(&name_text.as_ref())) {
                collect(root, &child, skip_excluded, found)?;
            }
        } else if kind.is_file() {
            if !(skip_excluded && SKIPPED_SUFFIXES.iter().any(|s| name_text.ends_with(s))) {
                found.push(child);
            }
        } else {
            return Err(BackupError::Unsupported(root.join(child)));
        }
    }
    Ok(())
}

fn is_key(relative: &Path) -> bool {
    relative
        .file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(KEY_SUFFIX))
}

fn portable(relative: &Path) -> String {
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

fn digest(path: &Path) -> Result<(String, u64), BackupError> {
    let mut file =
        fs::File::open(path).map_err(|e| BackupError::io(format!("read {}", path.display()), e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| BackupError::io(format!("read {}", path.display()), e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((hex::encode(hasher.finalize()), size))
}

fn create_private_dir(path: &Path) -> Result<(), BackupError> {
    fs::create_dir(path).map_err(|e| BackupError::io(format!("create {}", path.display()), e))?;
    make_private(path)
}

/// Creates `directory` and any missing parents below `base`, each owner-only.
fn create_private_dirs(base: &Path, directory: &Path) -> Result<(), BackupError> {
    let Ok(below) = directory.strip_prefix(base) else {
        return Ok(());
    };
    let mut current = base.to_path_buf();
    for component in below.components() {
        current.push(component);
        if !current.exists() {
            create_private_dir(&current)?;
        }
    }
    Ok(())
}

fn make_private(path: &Path) -> Result<(), BackupError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| BackupError::io(format!("restrict {}", path.display()), e))?;
    }
    let _ = path;
    Ok(())
}
