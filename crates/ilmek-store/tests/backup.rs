#![expect(
    clippy::unwrap_used,
    reason = "test helpers live outside #[test] fns, where clippy.toml's allow-unwrap-in-tests does not reach"
)]

use std::fs;
use std::path::Path;

use ilmek_store::{BackupError, MANIFEST_FILE, backup_tree, restore_tree, verify_backup};
use rusqlite::Connection;

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn read(root: &Path, relative: &str) -> Vec<u8> {
    fs::read(root.join(relative)).unwrap()
}

/// A WAL-mode database with committed rows; the returned connection stays open like a running
/// service's would.
fn live_database(path: &Path, rows: u32) -> Connection {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(path).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    connection
        .execute_batch("CREATE TABLE posts (id INTEGER PRIMARY KEY, body TEXT NOT NULL);")
        .unwrap();
    for n in 0..rows {
        connection
            .execute(
                "INSERT INTO posts (body) VALUES (?1)",
                [format!("post {n}")],
            )
            .unwrap();
    }
    connection
}

fn count(path: &Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM posts", [], |row| row.get(0))
        .unwrap()
}

fn source_tree(root: &Path) -> Connection {
    write(root, "community.json", b"{\"name\":\"demo\"}");
    write(root, "ca.crt.der", b"certificate");
    write(root, "ca.key.der", b"SECRET-KEY");
    write(root, "logs/host.log", b"noisy");
    write(root, "backups/old.db", b"older backup");
    write(root, "staging/half-written", b"partial");
    write(root, "apps/a1/versions/1.0.0/index.html", b"<p>hi</p>");
    live_database(&root.join("apps/a1/data/app.db"), 3)
}

#[test]
fn a_running_wal_database_and_files_survive_a_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let _running = source_tree(&source);

    let backup = dir.path().join("backup");
    let report = backup_tree(&source, &backup, false).unwrap();
    assert_eq!(report.databases, 1);
    assert_eq!(report.withheld_keys, ["ca.key.der"]);
    assert!(!report.keys_included);
    assert!(!backup.join("logs").exists());
    assert!(!backup.join("backups").exists());
    assert!(!backup.join("staging").exists());
    assert!(!backup.join("ca.key.der").exists());
    assert!(!dir.path().join("backup.partial").exists());
    verify_backup(&backup).unwrap();

    let restored = dir.path().join("restored");
    let outcome = restore_tree(&backup, &restored).unwrap();
    assert_eq!(outcome.withheld_keys, ["ca.key.der"]);
    assert_eq!(count(&restored.join("apps/a1/data/app.db")), 3);
    assert_eq!(read(&restored, "community.json"), b"{\"name\":\"demo\"}");
    assert_eq!(
        read(&restored, "apps/a1/versions/1.0.0/index.html"),
        b"<p>hi</p>"
    );
    assert!(!restored.join("ca.key.der").exists());
    assert!(!dir.path().join("restored.restoring").exists());
}

#[test]
fn keys_are_copied_only_when_asked_and_stay_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let _running = source_tree(&source);

    let backup = dir.path().join("backup");
    let report = backup_tree(&source, &backup, true).unwrap();
    assert!(report.keys_included);
    assert!(report.withheld_keys.is_empty());

    let restored = dir.path().join("restored");
    restore_tree(&backup, &restored).unwrap();
    assert_eq!(read(&restored, "ca.key.der"), b"SECRET-KEY");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&backup), 0o700);
        assert_eq!(mode(&backup.join("ca.key.der")), 0o600);
        assert_eq!(mode(&restored.join("ca.key.der")), 0o600);
        assert_eq!(mode(&restored), 0o700);
    }
}

#[test]
fn tampered_missing_or_extra_files_fail_verification_and_block_restore() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let _running = source_tree(&source);
    let backup = dir.path().join("backup");
    backup_tree(&source, &backup, false).unwrap();

    let flip = |relative: &str| {
        let mut bytes = read(&backup, relative);
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(backup.join(relative), bytes).unwrap();
    };
    flip("community.json");
    assert!(matches!(
        verify_backup(&backup),
        Err(BackupError::Mismatch { .. })
    ));
    assert!(restore_tree(&backup, &dir.path().join("r1")).is_err());
    assert!(!dir.path().join("r1").exists());
    flip("community.json");
    verify_backup(&backup).unwrap();

    flip("apps/a1/data/app.db");
    assert!(matches!(
        verify_backup(&backup),
        Err(BackupError::Mismatch { .. })
    ));
    flip("apps/a1/data/app.db");

    fs::remove_file(backup.join("ca.crt.der")).unwrap();
    assert!(matches!(
        verify_backup(&backup),
        Err(BackupError::Mismatch { .. })
    ));
    write(&backup, "ca.crt.der", b"certificate");
    verify_backup(&backup).unwrap();

    write(&backup, "intruder-wal", b"unlisted");
    assert!(matches!(
        verify_backup(&backup),
        Err(BackupError::Mismatch { .. })
    ));
}

#[test]
fn a_manifest_path_that_escapes_the_backup_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    write(&source, "a.txt", b"a");
    let backup = dir.path().join("backup");
    backup_tree(&source, &backup, false).unwrap();

    for hostile in ["../escape.txt", "/etc/passwd", "a/../../b", "", "dir\\file"] {
        let manifest = fs::read_to_string(backup.join(MANIFEST_FILE)).unwrap();
        let hostile_manifest = manifest.replace("\"a.txt\"", &format!("{hostile:?}"));
        let path = backup.join(MANIFEST_FILE);
        fs::write(&path, hostile_manifest).unwrap();
        assert!(
            matches!(verify_backup(&backup), Err(BackupError::Manifest(_))),
            "{hostile:?} must be refused"
        );
        fs::write(&path, manifest).unwrap();
    }
    verify_backup(&backup).unwrap();
}

#[test]
fn existing_destinations_are_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    write(&source, "a.txt", b"a");
    let backup = dir.path().join("backup");
    backup_tree(&source, &backup, false).unwrap();
    assert!(matches!(
        backup_tree(&source, &backup, false),
        Err(BackupError::DestinationExists(_))
    ));

    let occupied = dir.path().join("occupied");
    write(&occupied, "keep.txt", b"important");
    assert!(matches!(
        restore_tree(&backup, &occupied),
        Err(BackupError::DestinationNotEmpty(_))
    ));
    assert_eq!(read(&occupied, "keep.txt"), b"important");

    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    restore_tree(&backup, &empty).unwrap();
    assert_eq!(read(&empty, "a.txt"), b"a");
}

#[test]
fn an_unsupported_format_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    write(&source, "a.txt", b"a");
    let backup = dir.path().join("backup");
    backup_tree(&source, &backup, false).unwrap();
    let path = backup.join(MANIFEST_FILE);
    let manifest = fs::read_to_string(&path).unwrap();
    fs::write(&path, manifest.replace("\"format\": 1", "\"format\": 99")).unwrap();
    assert!(matches!(
        verify_backup(&backup),
        Err(BackupError::Manifest(_))
    ));
}

#[cfg(unix)]
#[test]
fn symbolic_links_fail_the_backup_and_leave_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    write(&source, "a.txt", b"a");
    std::os::unix::fs::symlink("/etc/hostname", source.join("link")).unwrap();
    let backup = dir.path().join("backup");
    assert!(matches!(
        backup_tree(&source, &backup, false),
        Err(BackupError::Unsupported(_))
    ));
    assert!(!backup.exists());
    assert!(!dir.path().join("backup.partial").exists());
}
