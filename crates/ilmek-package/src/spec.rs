use std::path::Path;

use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{Name, PackagePath, Text, Version};
use serde::{Deserialize, Serialize};

use crate::error::PackageError;

/// `app.toml`: what a developer writes; `pack` turns it into a signed manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSpec {
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
}

impl AppSpec {
    pub fn load(path: &Path) -> Result<Self, PackageError> {
        let text = std::fs::read_to_string(path).map_err(|e| PackageError::io(path, e))?;
        toml::from_str(&text).map_err(|e| PackageError::Source(format!("app.toml: {e}")))
    }

    pub fn to_toml(&self) -> Result<String, PackageError> {
        toml::to_string_pretty(self).map_err(|e| PackageError::Source(e.to_string()))
    }
}
