#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::fs;

use ilmek_package::{PackageError, PackageLimits, copy_package, init_app, pack, verify_dir};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::Hash;
use tempfile::{TempDir, tempdir};

struct Built {
    _root: TempDir,
    source: std::path::PathBuf,
    package: std::path::PathBuf,
}

fn build() -> Built {
    let root = tempdir().unwrap();
    let source = root.path().join("src");
    init_app(
        &source,
        "nuri".parse().unwrap(),
        "forum".parse().unwrap(),
        Uuid7::generate(),
    )
    .unwrap();
    fs::create_dir_all(source.join("assets/js")).unwrap();
    fs::write(source.join("assets/js/app.js"), "console.log('hi')").unwrap();
    fs::write(source.join("assets/style.css"), "body{}").unwrap();
    let package = root.path().join("pkg");
    pack(&source, &package).unwrap();
    Built {
        _root: root,
        source,
        package,
    }
}

fn verify(built: &Built) -> Result<ilmek_package::Package, PackageError> {
    verify_dir(&built.package, None, PackageLimits::default())
}

#[test]
fn a_packed_package_verifies_and_lists_every_file_sorted() {
    let built = build();
    let package = verify(&built).unwrap();
    let paths: Vec<_> = package
        .manifest
        .files
        .iter()
        .map(|f| f.path.to_string())
        .collect();
    assert_eq!(paths, ["index.html", "js/app.js", "style.css"]);
    assert_eq!(
        package.manifest.total_size_bytes,
        package
            .manifest
            .files
            .iter()
            .map(|f| f.size_bytes)
            .sum::<u64>()
    );
    assert_eq!(
        fs::read(built.package.join("manifest.json")).unwrap(),
        package.manifest_bytes
    );
}

#[test]
fn packing_the_same_source_twice_gives_identical_manifest_bytes() {
    let built = build();
    let again = built.package.with_file_name("pkg2");
    pack(&built.source, &again).unwrap();
    assert_eq!(
        fs::read(built.package.join("manifest.json")).unwrap(),
        fs::read(again.join("manifest.json")).unwrap()
    );
}

#[test]
fn a_modified_file_of_the_same_size_is_caught_by_its_hash() {
    let built = build();
    fs::write(built.package.join("assets/style.css"), "body{;").unwrap();
    assert!(matches!(verify(&built), Err(PackageError::HashMismatch(p)) if p == "style.css"));
}

#[test]
fn size_changes_missing_files_and_extra_files_are_caught() {
    let built = build();
    fs::write(
        built.package.join("assets/style.css"),
        "body{} /* longer */",
    )
    .unwrap();
    assert!(matches!(
        verify(&built),
        Err(PackageError::SizeMismatch { .. })
    ));

    let built = build();
    fs::remove_file(built.package.join("assets/js/app.js")).unwrap();
    assert!(matches!(verify(&built), Err(PackageError::Missing(_))));

    let built = build();
    fs::write(built.package.join("assets/backdoor.js"), "x").unwrap();
    assert!(matches!(verify(&built), Err(PackageError::Unlisted(p)) if p == "backdoor.js"));
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_smuggle_files_in() {
    let built = build();
    fs::remove_file(built.package.join("assets/style.css")).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", built.package.join("assets/style.css")).unwrap();
    assert!(matches!(verify(&built), Err(PackageError::NotRegular(_))));
}

#[test]
fn a_pinned_manifest_hash_must_match() {
    let built = build();
    let good = verify(&built).unwrap().manifest_hash;
    assert!(verify_dir(&built.package, Some(good), PackageLimits::default()).is_ok());
    let wrong = Hash::from_bytes([1; 32]);
    assert!(matches!(
        verify_dir(&built.package, Some(wrong), PackageLimits::default()),
        Err(PackageError::ManifestHash { .. })
    ));
}

#[test]
fn packing_rejects_unsafe_names_and_refuses_to_overwrite() {
    let built = build();
    fs::write(built.source.join("assets/CON.txt"), "x").unwrap();
    let out = built.package.with_file_name("bad");
    assert!(matches!(
        pack(&built.source, &out),
        Err(PackageError::Source(_))
    ));
    assert!(
        !out.exists(),
        "nothing may be written for a rejected package"
    );

    fs::remove_file(built.source.join("assets/CON.txt")).unwrap();
    assert!(matches!(
        pack(&built.source, &built.package),
        Err(PackageError::DestinationExists(_))
    ));
}

#[test]
fn the_package_limit_is_enforced_when_verifying() {
    let built = build();
    let tiny = PackageLimits {
        max_total_bytes: 10,
    };
    let result = verify_dir(&built.package, None, tiny);
    assert!(matches!(result, Err(PackageError::Manifest(_))));
}

#[test]
fn copies_are_complete_and_never_overwrite() {
    let built = build();
    let target = built.package.with_file_name("copy");
    copy_package(&built.package, &target).unwrap();
    verify_dir(&target, None, PackageLimits::default()).unwrap();
    assert!(matches!(
        copy_package(&built.package, &target),
        Err(PackageError::DestinationExists(_))
    ));
}

#[test]
fn a_changed_manifest_no_longer_matches_the_pinned_hash() {
    let built = build();
    let good = verify(&built).unwrap().manifest_hash;
    let manifest = built.package.join("manifest.json");
    let mut bytes = fs::read(&manifest).unwrap();
    bytes.push(b' ');
    fs::write(&manifest, bytes).unwrap();
    assert!(matches!(
        verify_dir(&built.package, Some(good), PackageLimits::default()),
        Err(PackageError::ManifestHash { .. })
    ));
}
