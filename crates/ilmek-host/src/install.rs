//! Package installation: stage, verify, then activate atomically (REQUIREMENTS §17.5).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ilmek_core::clock::Clock;
use ilmek_package::{Package, PackageLimits, copy_package, verify_dir};
use ilmek_protocol::Uuid7;
use ilmek_protocol::strict_json::{Profile, parse_object_with};
use ilmek_protocol::wire::{Hash, Version};

use crate::error::HostError;
use crate::store::{AppRow, Desired, Store, VersionRow};

/// The runtime version this host implements; manifests may require at most this.
pub const RUNTIME_VERSION: Version = Version::new(1, 0, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installed {
    pub app_id: Uuid7,
    pub version: Version,
    pub manifest_hash: Hash,
    /// `false` when the exact package was already installed.
    pub changed: bool,
}

#[derive(Clone)]
pub struct Installer {
    data_dir: PathBuf,
    store: Store,
    limits: PackageLimits,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Installer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Installer")
            .field("data_dir", &self.data_dir)
            .finish_non_exhaustive()
    }
}

impl Installer {
    #[must_use]
    pub fn new(
        data_dir: PathBuf,
        store: Store,
        max_package_bytes: u64,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            data_dir,
            store,
            limits: PackageLimits {
                max_total_bytes: max_package_bytes,
            },
            clock,
        }
    }

    #[must_use]
    pub fn app_dir(&self, app_id: Uuid7) -> PathBuf {
        self.data_dir.join("apps").join(app_id.to_string())
    }

    #[must_use]
    pub fn data_dir_of(&self, app_id: Uuid7) -> PathBuf {
        self.app_dir(app_id).join("data")
    }

    fn staging_root(&self) -> PathBuf {
        self.data_dir.join("staging")
    }

    /// Installs an application that is not on this host yet.
    pub async fn install(&self, package_dir: &Path) -> Result<Installed, HostError> {
        let staged = self.stage(package_dir).await?;
        let outcome = self.install_staged(&staged).await;
        staged.discard();
        outcome
    }

    async fn install_staged(&self, staged: &Staged) -> Result<Installed, HostError> {
        let manifest = &staged.package.manifest;
        if self.store.app(manifest.app_id).await?.is_some() {
            return Err(HostError::Conflict("already installed; use update"));
        }
        if manifest.min_runtime_version > RUNTIME_VERSION {
            return Err(HostError::Conflict("package needs a newer runtime"));
        }
        let now = self.clock.now_ms();
        let version_row = self.place(staged)?;
        let app = AppRow {
            app_id: manifest.app_id,
            namespace: manifest.namespace.clone(),
            slug: manifest.slug.clone(),
            publisher_id: manifest.publisher_id,
            name: manifest.name.to_string(),
            backend_required: manifest.backend_required,
            active_version: manifest.version,
            desired: Desired::Stopped,
            restart_generation: 0,
        };
        if let Err(error) = self.store.insert_app(app, version_row.clone(), now).await {
            let _ = std::fs::remove_dir_all(&version_row.directory);
            return Err(error);
        }
        self.ensure_data_dir(manifest.app_id)?;
        Ok(staged.installed(true))
    }

    /// Verifies and activates a newer version of an installed application. The running
    /// version is untouched unless every check passes.
    pub async fn update(&self, package_dir: &Path) -> Result<Installed, HostError> {
        let staged = self.stage(package_dir).await?;
        let outcome = self.update_staged(&staged).await;
        staged.discard();
        outcome
    }

    async fn update_staged(&self, staged: &Staged) -> Result<Installed, HostError> {
        let manifest = &staged.package.manifest;
        let current = self
            .store
            .app(manifest.app_id)
            .await?
            .ok_or(HostError::UnknownApp)?;
        if current.namespace != manifest.namespace
            || current.slug != manifest.slug
            || current.publisher_id != manifest.publisher_id
        {
            return Err(HostError::Conflict(
                "package identity differs from the installed application",
            ));
        }
        if manifest.min_runtime_version > RUNTIME_VERSION {
            return Err(HostError::Conflict("package needs a newer runtime"));
        }
        if let Some(existing) = self
            .store
            .version(manifest.app_id, manifest.version)
            .await?
        {
            return if existing.manifest_hash == staged.package.manifest_hash {
                Ok(staged.installed(false))
            } else {
                Err(HostError::Conflict(
                    "this version is already installed with other content",
                ))
            };
        }
        if manifest.version <= current.active_version {
            return Err(HostError::Conflict(
                "update must be newer than the active version",
            ));
        }
        let version_row = self.place(staged)?;
        let activated = self
            .store
            .activate_new_version(
                manifest.app_id,
                version_row.clone(),
                manifest.name.to_string(),
                self.clock.now_ms(),
            )
            .await;
        if let Err(error) = activated {
            let _ = std::fs::remove_dir_all(&version_row.directory);
            return Err(error);
        }
        Ok(staged.installed(true))
    }

    /// Makes an already installed version active again.
    pub async fn rollback(&self, app_id: Uuid7, version: Version) -> Result<(), HostError> {
        self.store.set_active_version(app_id, version).await
    }

    /// Removes an application. Data is kept unless `purge_data` is set explicitly.
    pub async fn remove(&self, app_id: Uuid7, purge_data: bool) -> Result<(), HostError> {
        let app = self.store.app(app_id).await?.ok_or(HostError::UnknownApp)?;
        if app.desired == Desired::Running {
            return Err(HostError::Conflict(
                "stop the application before removing it",
            ));
        }
        self.store.remove_app(app_id).await?;
        let root = self.app_dir(app_id);
        remove_dir_if_present(&root.join("versions"))?;
        if purge_data {
            remove_dir_if_present(&root)?;
        }
        Ok(())
    }

    /// Deletes staging leftovers and version directories the database does not know about.
    /// Run at start-up to recover from a crash during installation.
    pub async fn clean_leftovers(&self) -> Result<(), HostError> {
        remove_dir_if_present(&self.staging_root())?;
        for app in self.store.apps().await? {
            let versions_dir = self.app_dir(app.app_id).join("versions");
            let Ok(entries) = std::fs::read_dir(&versions_dir) else {
                continue;
            };
            let known: Vec<PathBuf> = self
                .store
                .versions(app.app_id)
                .await?
                .into_iter()
                .map(|v| v.directory)
                .collect();
            for entry in entries.flatten() {
                if !known.contains(&entry.path()) {
                    remove_dir_if_present(&entry.path())?;
                }
            }
        }
        Ok(())
    }

    fn ensure_data_dir(&self, app_id: Uuid7) -> Result<(), HostError> {
        let dir = self.data_dir_of(app_id);
        std::fs::create_dir_all(&dir).map_err(|e| HostError::io(&dir, e))
    }

    /// Copies the package into a private staging directory and verifies the copy.
    async fn stage(&self, package_dir: &Path) -> Result<Staged, HostError> {
        let key_id = declared_key_id(package_dir)?;
        let key = self
            .store
            .trusted_publisher(key_id)
            .await?
            .ok_or_else(|| HostError::UntrustedPublisher(key_id.to_string()))?;

        let root = self.staging_root();
        std::fs::create_dir_all(&root).map_err(|e| HostError::io(&root, e))?;
        let staging = root.join(Uuid7::generate().to_string());
        copy_package(package_dir, &staging)?;
        match verify_dir(&staging, &key, None, self.limits) {
            Ok(package) => Ok(Staged { package }),
            Err(error) => {
                let _ = std::fs::remove_dir_all(&staging);
                Err(error.into())
            }
        }
    }

    /// Moves a verified staged package to its permanent, immutable location.
    fn place(&self, staged: &Staged) -> Result<VersionRow, HostError> {
        let manifest = &staged.package.manifest;
        let name = format!(
            "{}-{}",
            manifest.version,
            &staged.package.manifest_hash.to_string()[..12]
        );
        let directory = self.app_dir(manifest.app_id).join("versions").join(name);
        if let Some(parent) = directory.parent() {
            std::fs::create_dir_all(parent).map_err(|e| HostError::io(parent, e))?;
        }
        std::fs::rename(&staged.package.dir, &directory)
            .map_err(|e| HostError::io(&directory, e))?;
        Ok(VersionRow {
            version: manifest.version,
            manifest_hash: staged.package.manifest_hash,
            directory,
        })
    }
}

struct Staged {
    package: Package,
}

impl Staged {
    /// Removes the staging directory if it was not moved into place.
    fn discard(&self) {
        let _ = std::fs::remove_dir_all(&self.package.dir);
    }

    fn installed(&self, changed: bool) -> Installed {
        Installed {
            app_id: self.package.manifest.app_id,
            version: self.package.manifest.version,
            manifest_hash: self.package.manifest_hash,
            changed,
        }
    }
}

/// Reads the claimed publisher key id so the right trusted key can verify the package.
/// Nothing else in the manifest is believed until the signature has been checked.
fn declared_key_id(package_dir: &Path) -> Result<Hash, HostError> {
    let path = package_dir.join(ilmek_package::MANIFEST_FILE);
    let metadata = std::fs::metadata(&path).map_err(|e| HostError::io(&path, e))?;
    if metadata.len() > ilmek_protocol::manifest::MAX_MANIFEST_BYTES as u64 {
        return Err(HostError::Invalid("manifest too large"));
    }
    let bytes = std::fs::read(&path).map_err(|e| HostError::io(&path, e))?;
    let object = parse_object_with(&bytes, Profile::Body)
        .map_err(|_| HostError::Invalid("manifest is not valid JSON"))?;
    object
        .get("publisher_key_id")
        .and_then(|value| value.as_str())
        .and_then(|text| text.parse().ok())
        .ok_or(HostError::Invalid("manifest has no publisher_key_id"))
}

fn remove_dir_if_present(path: &Path) -> Result<(), HostError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(HostError::io(path, error)),
    }
}
