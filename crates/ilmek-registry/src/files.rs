use std::path::{Path, PathBuf};

use crate::error::RegistryError;

pub(crate) struct Layout {
    root: PathBuf,
}

impl Layout {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn profile(&self) -> PathBuf {
        self.root.join("community.json")
    }

    pub(crate) fn database(&self) -> PathBuf {
        self.root.join("registry.db")
    }
}

fn io_error(error: &std::io::Error) -> RegistryError {
    RegistryError::Internal(format!("file access failed: {error}"))
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, RegistryError> {
    std::fs::read(path).map_err(|e| io_error(&e))
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<(), RegistryError> {
    std::fs::write(path, bytes).map_err(|e| io_error(&e))
}

pub(crate) fn create_dir(path: &Path) -> Result<(), RegistryError> {
    std::fs::create_dir_all(path).map_err(|e| io_error(&e))
}
