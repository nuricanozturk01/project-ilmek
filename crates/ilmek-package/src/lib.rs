//! Application packages on disk: `manifest.json`, `manifest.sig` and `assets/`.

mod error;
mod hashing;
mod pack;
mod spec;
mod verify;

pub use error::PackageError;
pub use hashing::{FileDigest, hash_file};
pub use pack::{PackedPackage, init_app, pack};
pub use spec::AppSpec;
pub use verify::{Package, PackageLimits, copy_package, verify_dir, verify_manifest};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const SIGNATURE_FILE: &str = "manifest.sig";
pub const ASSETS_DIR: &str = "assets";
