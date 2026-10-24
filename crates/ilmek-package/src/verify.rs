use std::fs;
use std::path::{Path, PathBuf};

use ilmek_protocol::digest::sha256;
use ilmek_protocol::manifest::{DEFAULT_MAX_TOTAL_BYTES, MAX_MANIFEST_BYTES, Manifest};
use ilmek_protocol::wire::Hash;

use crate::error::PackageError;
use crate::hashing::hash_file;
use crate::pack::collect;
use crate::{ASSETS_DIR, MANIFEST_FILE};

#[derive(Debug, Clone, Copy)]
pub struct PackageLimits {
    pub max_total_bytes: u64,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
        }
    }
}

/// A package that passed every check.
#[derive(Debug, Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub manifest_bytes: Vec<u8>,
    pub manifest_hash: Hash,
}

impl Package {
    #[must_use]
    pub fn asset_path(&self, path: &ilmek_protocol::wire::PackagePath) -> PathBuf {
        self.dir.join(ASSETS_DIR).join(path.as_str())
    }
}

/// Checks manifest bytes: size, hash against `expected_hash`, schema.
pub fn verify_manifest(
    manifest_bytes: &[u8],
    expected_hash: Option<Hash>,
    limits: PackageLimits,
) -> Result<(Manifest, Hash), PackageError> {
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ilmek_protocol::manifest::ManifestError::TooLarge.into());
    }
    let hash = Hash::from_bytes(sha256(manifest_bytes));
    if let Some(expected) = expected_hash.filter(|expected| *expected != hash) {
        return Err(PackageError::ManifestHash {
            expected: expected.to_string(),
        });
    }
    let manifest = Manifest::parse(manifest_bytes, limits.max_total_bytes)?;
    Ok((manifest, hash))
}

/// Fully verifies a package directory: manifest, then every file's size and hash.
/// Files on disk that the manifest does not list are an error.
pub fn verify_dir(
    dir: &Path,
    expected_hash: Option<Hash>,
    limits: PackageLimits,
) -> Result<Package, PackageError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let size = fs::metadata(&manifest_path)
        .map_err(|e| PackageError::io(&manifest_path, e))?
        .len();
    if size > MAX_MANIFEST_BYTES as u64 {
        return Err(ilmek_protocol::manifest::ManifestError::TooLarge.into());
    }
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|e| PackageError::io(&manifest_path, e))?;
    let (manifest, manifest_hash) = verify_manifest(&manifest_bytes, expected_hash, limits)?;

    let assets_root = dir.join(ASSETS_DIR);
    let mut on_disk = Vec::new();
    collect(&assets_root, &assets_root, &mut on_disk)?;
    for (path, _) in &on_disk {
        if !manifest.files.iter().any(|f| &f.path == path) {
            return Err(PackageError::Unlisted(path.to_string()));
        }
    }
    for file in &manifest.files {
        let disk = assets_root.join(file.path.as_str());
        let metadata = fs::symlink_metadata(&disk)
            .map_err(|_| PackageError::Missing(file.path.to_string()))?;
        if !metadata.file_type().is_file() {
            return Err(PackageError::NotRegular(file.path.to_string()));
        }
        let digest = hash_file(&disk)?;
        if digest.size != file.size_bytes {
            return Err(PackageError::SizeMismatch {
                path: file.path.to_string(),
                declared: file.size_bytes,
                actual: digest.size,
            });
        }
        if digest.sha256 != file.sha256 {
            return Err(PackageError::HashMismatch(file.path.to_string()));
        }
    }
    Ok(Package {
        dir: dir.to_path_buf(),
        manifest,
        manifest_bytes,
        manifest_hash,
    })
}

/// Copies a package tree (regular files only) to a new location.
pub fn copy_package(source: &Path, destination: &Path) -> Result<(), PackageError> {
    if destination.exists() {
        return Err(PackageError::DestinationExists(
            destination.display().to_string(),
        ));
    }
    copy_tree(source, destination)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), PackageError> {
    fs::create_dir_all(destination).map_err(|e| PackageError::io(destination, e))?;
    for entry in fs::read_dir(source).map_err(|e| PackageError::io(source, e))? {
        let entry = entry.map_err(|e| PackageError::io(source, e))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let kind = fs::symlink_metadata(&from)
            .map_err(|e| PackageError::io(&from, e))?
            .file_type();
        if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to).map_err(|e| PackageError::io(&to, e))?;
        } else {
            return Err(PackageError::NotRegular(from.display().to_string()));
        }
    }
    Ok(())
}
