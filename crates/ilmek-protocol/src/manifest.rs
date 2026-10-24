//! Package manifest schema and validation (PROTOCOL.md §15).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::ids::Uuid7;
use crate::strict_json::{self, JsonError, Profile};
use crate::wire::{Hash, Name, PackagePath, Text, Version};

pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_FILES: usize = 1024;
/// Default cap on the sum of all file sizes; operators may raise it locally.
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    #[serde(rename = "app.call")]
    AppCall,
    #[serde(rename = "app.subscribe")]
    AppSubscribe,
    #[serde(rename = "storage.local")]
    StorageLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub path: PackagePath,
    pub size_bytes: u64,
    pub sha256: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub app_id: Uuid7,
    pub publisher_id: Uuid7,
    pub namespace: Name,
    pub slug: Name,
    pub name: Text<128>,
    pub description: Text<2048>,
    pub version: Version,
    pub min_runtime_version: Version,
    pub entrypoint: PackagePath,
    pub permissions: Vec<Permission>,
    pub backend_required: bool,
    pub files: Vec<ManifestFile>,
    pub total_size_bytes: u64,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest larger than {MAX_MANIFEST_BYTES} bytes")]
    TooLarge,
    #[error("manifest is not strict JSON: {0}")]
    Json(#[from] JsonError),
    #[error("manifest does not match the schema: {0}")]
    Schema(String),
    #[error("unsupported manifest schema version {0}")]
    SchemaVersion(u32),
    #[error("duplicate permission")]
    DuplicatePermission,
    #[error("more than {MAX_FILES} files")]
    TooManyFiles,
    #[error("files must be unique and sorted by path bytes")]
    FileOrder,
    #[error("paths collide when case is ignored or one path is a directory of another")]
    PathCollision,
    #[error("entrypoint must be a listed .html file")]
    Entrypoint,
    #[error("total_size_bytes does not equal the sum of file sizes")]
    TotalSize,
    #[error("package larger than the allowed {0} bytes")]
    OverLimit(u64),
}

impl Manifest {
    /// Parses and validates the exact stored bytes of `manifest.json`.
    pub fn parse(bytes: &[u8], max_total_bytes: u64) -> Result<Self, ManifestError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(ManifestError::TooLarge);
        }
        let object = strict_json::parse_object_with(bytes, Profile::Body)?;
        let manifest: Self = serde_json::from_value(Value::Object(object))
            .map_err(|e| ManifestError::Schema(e.to_string()))?;
        manifest.validate(max_total_bytes)?;
        Ok(manifest)
    }

    fn validate(&self, max_total_bytes: u64) -> Result<(), ManifestError> {
        if self.schema_version != 1 {
            return Err(ManifestError::SchemaVersion(self.schema_version));
        }
        let distinct: HashSet<_> = self.permissions.iter().collect();
        if distinct.len() != self.permissions.len() {
            return Err(ManifestError::DuplicatePermission);
        }
        if self.files.len() > MAX_FILES {
            return Err(ManifestError::TooManyFiles);
        }
        let sorted = self
            .files
            .windows(2)
            .all(|pair| pair[0].path.as_str() < pair[1].path.as_str());
        if !sorted {
            return Err(ManifestError::FileOrder);
        }
        self.check_collisions()?;
        let listed = self.files.iter().any(|f| f.path == self.entrypoint);
        let html = self
            .entrypoint
            .as_str()
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext == "html");
        if !listed || !html {
            return Err(ManifestError::Entrypoint);
        }
        let total = self
            .files
            .iter()
            .try_fold(0u64, |sum, file| sum.checked_add(file.size_bytes))
            .ok_or(ManifestError::TotalSize)?;
        if total != self.total_size_bytes {
            return Err(ManifestError::TotalSize);
        }
        if total > max_total_bytes {
            return Err(ManifestError::OverLimit(max_total_bytes));
        }
        Ok(())
    }

    fn check_collisions(&self) -> Result<(), ManifestError> {
        let mut folded = HashSet::new();
        let mut directories = HashSet::new();
        for file in &self.files {
            let lowered = file.path.as_str().to_ascii_lowercase();
            if !folded.insert(lowered.clone()) {
                return Err(ManifestError::PathCollision);
            }
            let mut prefix = String::new();
            for segment in lowered.split('/').take(lowered.matches('/').count()) {
                prefix.push_str(segment);
                directories.insert(prefix.clone());
                prefix.push('/');
            }
        }
        if folded.iter().any(|path| directories.contains(path)) {
            return Err(ManifestError::PathCollision);
        }
        Ok(())
    }

    #[must_use]
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}
