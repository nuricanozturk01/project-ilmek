use std::fs;
use std::path::{Path, PathBuf};

use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::sha256;
use ilmek_protocol::manifest::{DEFAULT_MAX_TOTAL_BYTES, Manifest, ManifestFile, Permission};
use ilmek_protocol::wire::{Hash, Name, PackagePath, Version};

use crate::error::PackageError;
use crate::hashing::hash_file;
use crate::spec::AppSpec;
use crate::{ASSETS_DIR, MANIFEST_FILE};

#[derive(Debug)]
pub struct PackedPackage {
    pub manifest: Manifest,
    pub manifest_hash: Hash,
    pub output: PathBuf,
}

/// Creates a new application project: `app.toml` and a starter `assets/index.html`.
pub fn init_app(
    dir: &Path,
    namespace: Name,
    slug: Name,
    publisher_id: Uuid7,
) -> Result<AppSpec, PackageError> {
    if dir.exists() {
        return Err(PackageError::DestinationExists(dir.display().to_string()));
    }
    let assets = dir.join(ASSETS_DIR);
    fs::create_dir_all(&assets).map_err(|e| PackageError::io(&assets, e))?;
    let spec = AppSpec {
        app_id: Uuid7::generate(),
        publisher_id,
        name: slug
            .as_str()
            .parse()
            .map_err(|_| PackageError::Source("name".into()))?,
        description: String::new()
            .parse()
            .map_err(|_| PackageError::Source("description".into()))?,
        namespace,
        slug,
        version: Version::new(1, 0, 0),
        min_runtime_version: Version::new(1, 0, 0),
        entrypoint: "index.html"
            .parse()
            .map_err(|_| PackageError::Source("entrypoint".into()))?,
        permissions: vec![Permission::AppCall],
        backend_required: false,
    };
    let toml = dir.join("app.toml");
    fs::write(&toml, spec.to_toml()?).map_err(|e| PackageError::io(&toml, e))?;
    let index = assets.join("index.html");
    fs::write(&index, STARTER_HTML).map_err(|e| PackageError::io(&index, e))?;
    Ok(spec)
}

const STARTER_HTML: &str = "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>App</title></head>\n<body><h1>Hello from İlmek</h1></body>\n</html>\n";

/// Builds `manifest.json` and copies the assets into `output`.
///
/// `source` must contain `app.toml` and an `assets/` directory.
pub fn pack(source: &Path, output: &Path) -> Result<PackedPackage, PackageError> {
    let spec = AppSpec::load(&source.join("app.toml"))?;
    pack_spec(spec, source, output)
}

/// Like [`pack`] but with an explicit spec, e.g. after overriding the publisher id.
pub fn pack_spec(
    spec: AppSpec,
    source: &Path,
    output: &Path,
) -> Result<PackedPackage, PackageError> {
    let assets_root = source.join(ASSETS_DIR);
    let mut found = Vec::new();
    collect(&assets_root, &assets_root, &mut found)?;
    found.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    let mut files = Vec::with_capacity(found.len());
    for (path, disk) in &found {
        let digest = hash_file(disk)?;
        files.push(ManifestFile {
            path: path.clone(),
            size_bytes: digest.size,
            sha256: digest.sha256,
        });
    }
    let total_size_bytes = files.iter().map(|f| f.size_bytes).sum();
    let manifest = Manifest {
        schema_version: 1,
        app_id: spec.app_id,
        publisher_id: spec.publisher_id,
        namespace: spec.namespace,
        slug: spec.slug,
        name: spec.name,
        description: spec.description,
        version: spec.version,
        min_runtime_version: spec.min_runtime_version,
        entrypoint: spec.entrypoint,
        permissions: spec.permissions,
        backend_required: spec.backend_required,
        files,
        total_size_bytes,
    };
    let manifest_bytes =
        serde_json::to_vec(&manifest).map_err(|e| PackageError::Source(e.to_string()))?;
    let manifest = Manifest::parse(&manifest_bytes, DEFAULT_MAX_TOTAL_BYTES)?;

    if output.exists() {
        return Err(PackageError::DestinationExists(
            output.display().to_string(),
        ));
    }
    fs::create_dir_all(output.join(ASSETS_DIR)).map_err(|e| PackageError::io(output, e))?;
    for (path, disk) in &found {
        let target = output.join(ASSETS_DIR).join(path.as_str());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| PackageError::io(parent, e))?;
        }
        fs::copy(disk, &target).map_err(|e| PackageError::io(&target, e))?;
    }
    write(&output.join(MANIFEST_FILE), &manifest_bytes)?;

    Ok(PackedPackage {
        manifest,
        manifest_hash: Hash::from_bytes(sha256(&manifest_bytes)),
        output: output.to_path_buf(),
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), PackageError> {
    fs::write(path, bytes).map_err(|e| PackageError::io(path, e))
}

/// Lists regular files below `dir` with their package-relative paths.
pub(crate) fn collect(
    root: &Path,
    dir: &Path,
    found: &mut Vec<(PackagePath, PathBuf)>,
) -> Result<(), PackageError> {
    let entries = fs::read_dir(dir).map_err(|e| PackageError::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| PackageError::io(dir, e))?;
        let path = entry.path();
        let kind = fs::symlink_metadata(&path)
            .map_err(|e| PackageError::io(&path, e))?
            .file_type();
        if kind.is_dir() {
            collect(root, &path, found)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| PackageError::NotRegular(path.display().to_string()))?;
            let text = relative
                .components()
                .map(|c| c.as_os_str().to_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .map(|parts| parts.join("/"))
                .ok_or_else(|| PackageError::NotRegular(path.display().to_string()))?;
            let package_path = text.parse().map_err(|_| {
                PackageError::Source(format!("`{text}` is not a valid package path"))
            })?;
            found.push((package_path, path));
        } else {
            return Err(PackageError::NotRegular(path.display().to_string()));
        }
    }
    Ok(())
}
