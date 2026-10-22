#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::sync::Arc;

use ilmek_client::{AppClient, Error};
use ilmek_example_notes::{MIGRATIONS, NotesBackend};
use ilmek_host::{AppState, BackendRow, Desired, publish};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{ErrorCode, Version};
use ilmek_testing::{Community, PackageSpec, backend_launcher, build_package, start_host};

struct Notes {
    _community: Community,
    alice: AppClient,
    alice_again: AppClient,
    bob: AppClient,
    _host: ilmek_testing::TestHost,
    _scratch: tempfile::TempDir,
}

async fn notes() -> Notes {
    let community = Community::start().await;
    let owner = community.publisher("nuri").await;
    let (launcher, _handles) = backend_launcher(MIGRATIONS, NotesBackend::new);
    let host = start_host(&community, &owner, Arc::new(launcher)).await;
    let scratch = tempfile::tempdir().unwrap();
    let spec = PackageSpec {
        namespace: "nuri",
        slug: "notlar",
        app_id: Uuid7::generate(),
        publisher_id: owner.id,
        version: Version::new(1, 0, 0),
        backend_required: true,
        permissions: vec![Permission::AppCall],
        files: vec![("index.html", b"<html></html>".to_vec())],
    };
    let package = build_package(scratch.path(), "notes", &spec, &owner.signer);
    let installed = host.installer.install(&package).await.unwrap();
    host.store
        .set_backend(
            installed.app_id,
            BackendRow {
                executable: "x".into(),
                args: vec![],
                accepted_versions: vec![],
            },
        )
        .await
        .unwrap();
    host.store
        .set_desired(installed.app_id, Desired::Running, false)
        .await
        .unwrap();
    loop {
        if host
            .store
            .status(installed.app_id)
            .await
            .unwrap()
            .is_some_and(|s| s.state == AppState::Running)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    let record = publish(
        &owner.member.connector,
        &host.store,
        installed.app_id,
        host.peer_id,
    )
    .await
    .unwrap();
    let alice_connector = Arc::new(community.member().await.connector);
    let alice = AppClient::new(Arc::clone(&alice_connector), record.clone(), record.version);
    let alice_again = AppClient::new(alice_connector, record.clone(), record.version);
    let bob = AppClient::new(
        Arc::new(community.member().await.connector),
        record.clone(),
        record.version,
    );
    Notes {
        _community: community,
        alice,
        alice_again,
        bob,
        _host: host,
        _scratch: scratch,
    }
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).unwrap()
}

fn code(error: &Error) -> Option<ErrorCode> {
    error.remote_code()
}

#[tokio::test(flavor = "multi_thread")]
async fn other_principals_cannot_read_change_or_even_detect_a_private_note() {
    let n = notes().await;
    let created = json(
        &n.alice
            .write(
                "notes.create",
                r#"{"title":"Sır","body":"gizli"}"#.as_bytes(),
                None,
            )
            .await
            .unwrap()
            .body,
    );
    let id = created["id"].as_i64().unwrap();

    let list = json(&n.bob.read("notes.list", b"{}").await.unwrap().body);
    assert!(list["notes"].as_array().unwrap().is_empty());
    let stolen = n
        .bob
        .read("notes.get", format!("{{\"id\":{id}}}").as_bytes())
        .await
        .unwrap_err();
    assert_eq!(code(&stolen), Some(ErrorCode::NotFound));
    let vandal = format!("{{\"id\":{id},\"expected_revision\":1,\"title\":\"x\",\"body\":\"y\"}}");
    assert_eq!(
        code(
            &n.bob
                .write("notes.update", vandal.as_bytes(), None)
                .await
                .unwrap_err()
        ),
        Some(ErrorCode::NotFound)
    );
    let delete = format!("{{\"id\":{id},\"expected_revision\":1}}");
    assert_eq!(
        code(
            &n.bob
                .write("notes.delete", delete.as_bytes(), None)
                .await
                .unwrap_err()
        ),
        Some(ErrorCode::NotFound)
    );

    let still = json(
        &n.alice
            .read("notes.get", format!("{{\"id\":{id}}}").as_bytes())
            .await
            .unwrap()
            .body,
    );
    assert_eq!(still["body"], "gizli");
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_edits_cannot_silently_overwrite_each_other() {
    let n = notes().await;
    let id = json(
        &n.alice
            .write("notes.create", br#"{"title":"Plan","body":"v1"}"#, None)
            .await
            .unwrap()
            .body,
    )["id"]
        .as_i64()
        .unwrap();
    let edit = |text: &str| {
        format!("{{\"id\":{id},\"expected_revision\":1,\"title\":\"Plan\",\"body\":\"{text}\"}}")
    };

    let winner = json(
        &n.alice
            .write("notes.update", edit("from phone").as_bytes(), None)
            .await
            .unwrap()
            .body,
    );
    assert_eq!(winner["revision"], 2);
    let loser = n
        .alice_again
        .write("notes.update", edit("from laptop").as_bytes(), None)
        .await
        .unwrap_err();
    assert_eq!(code(&loser), Some(ErrorCode::RevisionConflict));

    let kept = json(
        &n.alice
            .read("notes.get", format!("{{\"id\":{id}}}").as_bytes())
            .await
            .unwrap()
            .body,
    );
    assert_eq!(kept["body"], "from phone");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_retried_update_is_not_mistaken_for_a_conflict() {
    let n = notes().await;
    let id = json(
        &n.alice
            .write("notes.create", br#"{"title":"T","body":"b"}"#, None)
            .await
            .unwrap()
            .body,
    )["id"]
        .as_i64()
        .unwrap();
    let operation = Uuid7::generate();
    let body =
        format!("{{\"id\":{id},\"expected_revision\":1,\"title\":\"T\",\"body\":\"changed\"}}");
    let first = n
        .alice
        .write("notes.update", body.as_bytes(), Some(operation))
        .await
        .unwrap();
    let retry = n
        .alice
        .write("notes.update", body.as_bytes(), Some(operation))
        .await
        .unwrap();
    assert_eq!(
        first, retry,
        "the stored result is returned even though the revision has moved on"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_notes_are_rejected() {
    let n = notes().await;
    for body in [
        r#"{"title":"","body":"x"}"#,
        r#"{"body":"x"}"#,
        r#"{"title":"t","body":"x","extra":1}"#,
    ] {
        let error = n
            .alice
            .write("notes.create", body.as_bytes(), None)
            .await
            .unwrap_err();
        assert_eq!(code(&error), Some(ErrorCode::InvalidMessage), "{body}");
    }
}
