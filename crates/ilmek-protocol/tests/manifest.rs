#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use ilmek_protocol::manifest::{
    DEFAULT_MAX_TOTAL_BYTES, MAX_FILES, Manifest, ManifestError, Permission,
};
use serde::Deserialize;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("../../../tests/protocol/fixtures/wire_examples.ilmek.json");

#[derive(Deserialize)]
struct Fixtures {
    manifest_example: Example,
}

#[derive(Deserialize)]
struct Example {
    manifest_utf8: String,
}

const HASH: &str = "7377d7cda5afbb05ab3537a24f6ddbad0dad35186a616fe6f12dd62c138ddac9";

fn base() -> Value {
    serde_json::from_str::<Fixtures>(FIXTURE)
        .unwrap()
        .manifest_example
        .manifest_utf8
        .parse()
        .unwrap()
}

fn with_files(files: &[(&str, u64)]) -> Value {
    let mut manifest = base();
    manifest["files"] = Value::Array(
        files
            .iter()
            .map(|(p, s)| json!({"path": p, "size_bytes": s, "sha256": HASH}))
            .collect(),
    );
    manifest["total_size_bytes"] = json!(files.iter().map(|(_, s)| s).sum::<u64>());
    manifest
}

fn parse(value: &Value) -> Result<Manifest, ManifestError> {
    Manifest::parse(value.to_string().as_bytes(), DEFAULT_MAX_TOTAL_BYTES)
}

#[test]
fn the_fixture_manifest_is_valid() {
    let manifest = parse(&base()).unwrap();
    assert_eq!(manifest.entrypoint.as_str(), "index.html");
    assert_eq!(manifest.total_size_bytes, 65);
    assert!(!manifest.has_permission(Permission::AppCall));
}

#[test]
fn permissions_are_a_closed_set_without_duplicates() {
    let mut manifest = base();
    manifest["permissions"] = json!(["app.call", "storage.local"]);
    assert!(
        parse(&manifest)
            .unwrap()
            .has_permission(Permission::StorageLocal)
    );

    manifest["permissions"] = json!(["app.call", "app.call"]);
    assert!(matches!(
        parse(&manifest),
        Err(ManifestError::DuplicatePermission)
    ));
    manifest["permissions"] = json!(["fs.read"]);
    assert!(matches!(parse(&manifest), Err(ManifestError::Schema(_))));
}

#[test]
fn file_lists_must_be_sorted_unique_and_unambiguous() {
    let unsorted = with_files(&[("index.html", 1), ("a.css", 1)]);
    assert!(matches!(parse(&unsorted), Err(ManifestError::FileOrder)));

    let repeated = with_files(&[("index.html", 1), ("index.html", 1)]);
    assert!(matches!(parse(&repeated), Err(ManifestError::FileOrder)));

    let folded = with_files(&[("Style.css", 1), ("index.html", 1), ("style.css", 1)]);
    assert!(matches!(
        parse(&folded),
        Err(ManifestError::FileOrder | ManifestError::PathCollision)
    ));
    let collision = with_files(&[("a/B.js", 1), ("a/b.js", 1), ("index.html", 1)]);
    assert!(matches!(
        parse(&collision),
        Err(ManifestError::PathCollision)
    ));

    let file_and_dir = with_files(&[("a", 1), ("a/b.js", 1), ("index.html", 1)]);
    assert!(matches!(
        parse(&file_and_dir),
        Err(ManifestError::PathCollision)
    ));
}

#[test]
fn dangerous_paths_never_reach_the_manifest() {
    for path in [
        "../x.js",
        "/etc/passwd",
        "a\\b.js",
        "a//b.js",
        "c:.js",
        "nul.txt",
        "a/CON/x.js",
        "dot.",
        "",
    ] {
        let manifest = with_files(&[("index.html", 1), (path, 1)]);
        assert!(parse(&manifest).is_err(), "{path:?}");
    }
}

#[test]
fn entrypoint_and_sizes_are_checked() {
    let mut manifest = base();
    manifest["entrypoint"] = json!("missing.html");
    assert!(matches!(parse(&manifest), Err(ManifestError::Entrypoint)));

    let mut manifest = with_files(&[("app.js", 1), ("main.txt", 1)]);
    manifest["entrypoint"] = json!("main.txt");
    assert!(matches!(parse(&manifest), Err(ManifestError::Entrypoint)));

    let mut manifest = base();
    manifest["total_size_bytes"] = json!(1);
    assert!(matches!(parse(&manifest), Err(ManifestError::TotalSize)));

    let mut manifest = with_files(&[("index.html", 1), ("z.bin", 1)]);
    manifest["files"][0]["size_bytes"] = json!(u64::MAX);
    manifest["files"][1]["size_bytes"] = json!(5);
    assert!(matches!(parse(&manifest), Err(ManifestError::TotalSize)));

    let big = with_files(&[("index.html", DEFAULT_MAX_TOTAL_BYTES + 1)]);
    assert!(matches!(parse(&big), Err(ManifestError::OverLimit(_))));
}

#[test]
fn schema_strictness() {
    let mut manifest = base();
    manifest["schema_version"] = json!(2);
    assert!(matches!(
        parse(&manifest),
        Err(ManifestError::SchemaVersion(2))
    ));

    let mut manifest = base();
    manifest["surprise"] = json!(true);
    assert!(matches!(parse(&manifest), Err(ManifestError::Schema(_))));

    let duplicate = br#"{"schema_version":1,"schema_version":1}"#;
    assert!(matches!(
        Manifest::parse(duplicate, DEFAULT_MAX_TOTAL_BYTES),
        Err(ManifestError::Json(_))
    ));

    let names: Vec<(String, u64)> = (0..=MAX_FILES)
        .map(|n| (format!("f{n:05}.js"), 1))
        .collect();
    let refs: Vec<(&str, u64)> = names.iter().map(|(p, s)| (p.as_str(), *s)).collect();
    let mut many = with_files(&refs);
    many["entrypoint"] = json!("f00000.js");
    assert!(parse(&many).is_err());
    let too_many: Vec<(String, u64)> = std::iter::once(("index.html".to_owned(), 1))
        .chain((0..MAX_FILES).map(|n| (format!("j{n:05}.js"), 1)))
        .collect();
    let refs: Vec<(&str, u64)> = too_many.iter().map(|(p, s)| (p.as_str(), *s)).collect();
    assert!(matches!(
        parse(&with_files(&refs)),
        Err(ManifestError::TooManyFiles)
    ));
}

#[test]
fn oversized_manifests_are_refused_before_parsing() {
    let bytes = vec![b' '; 256 * 1024 + 1];
    assert!(matches!(
        Manifest::parse(&bytes, DEFAULT_MAX_TOTAL_BYTES),
        Err(ManifestError::TooLarge)
    ));
}
