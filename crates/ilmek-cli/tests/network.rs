//! Runs the real `ilmek` binary. Only read-only paths are exercised: nothing here needs root or
//! changes the machine.

#![expect(
    clippy::expect_used,
    reason = "the helper lives outside #[test] fns, where clippy.toml's allow-expect-in-tests does not reach"
)]

use std::process::{Command, Output};

const NODE_A: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../configs/node-a.toml.example"
);

fn ilmek(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ilmek"))
        .args(args)
        .output()
        .expect("the ilmek binary starts")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn plan_prints_the_changes_and_states_that_nothing_was_changed() {
    let out = ilmek(&["network", "plan", "--config", NODE_A]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("nothing has been changed"), "{stdout}");
    assert!(stdout.contains("ssid=ILMEK-A"), "{stdout}");
    assert!(stdout.contains("@PASSPHRASE@"), "{stdout}");
}

#[test]
fn apply_without_yes_prints_the_plan_and_refuses() {
    let out = ilmek(&["network", "apply", "--config", NODE_A]);
    assert!(!out.status.success());
    assert!(text(&out.stdout).contains("Plan for NODE-A"));
    assert!(text(&out.stderr).contains("nothing was changed"));
}

#[test]
fn status_and_rollback_on_an_empty_snapshot_directory() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().to_str().expect("utf-8 path");

    let status = ilmek(&["network", "status", "--snapshot-dir", path]);
    assert!(status.status.success());
    assert!(text(&status.stdout).contains("no snapshots"));

    let rollback = ilmek(&["network", "rollback", "--snapshot-dir", path]);
    assert!(!rollback.status.success());
    assert!(text(&rollback.stderr).contains("no snapshot to roll back"));
}

#[test]
fn a_missing_configuration_is_reported_not_panicked() {
    let out = ilmek(&["network", "plan", "--config", "/nonexistent/node.toml"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("cannot read"));
}
