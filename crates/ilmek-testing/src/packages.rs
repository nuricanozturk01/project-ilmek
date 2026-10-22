use std::fs;
use std::path::{Path, PathBuf};

use ilmek_identity::PublisherSigner;
use ilmek_package::{init_app, pack};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::Version;

/// What a test package should contain.
#[derive(Debug, Clone)]
pub struct PackageSpec<'a> {
    pub namespace: &'a str,
    pub slug: &'a str,
    pub app_id: Uuid7,
    pub publisher_id: Uuid7,
    pub version: Version,
    pub backend_required: bool,
    pub permissions: Vec<Permission>,
    pub files: Vec<(&'a str, Vec<u8>)>,
}

/// Packs and signs a package under `root` and returns its directory.
pub fn build_package(
    root: &Path,
    name: &str,
    spec: &PackageSpec<'_>,
    signer: &PublisherSigner,
) -> PathBuf {
    let source = root.join(format!("src-{name}"));
    let mut app = init_app(
        &source,
        spec.namespace.parse().unwrap(),
        spec.slug.parse().unwrap(),
        spec.publisher_id,
    )
    .unwrap();
    app.app_id = spec.app_id;
    app.version = spec.version;
    app.backend_required = spec.backend_required;
    app.permissions.clone_from(&spec.permissions);
    fs::write(source.join("app.toml"), app.to_toml().unwrap()).unwrap();
    for (path, bytes) in &spec.files {
        let target = source.join("assets").join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
    let out = root.join(format!("pkg-{name}"));
    pack(&source, signer, &out).unwrap();
    out
}
