#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::sync::Arc;

use ilmek_client::{AppClient, Error};
use ilmek_example_forum::{ForumBackend, MIGRATIONS};
use ilmek_host::{AppState, BackendRow, Desired, publish};
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::Permission;
use ilmek_protocol::wire::{ErrorCode, Version};
use ilmek_testing::{Community, PackageSpec, backend_launcher, build_package, start_host};

struct Forum {
    _community: Community,
    alice: AppClient,
    bob: AppClient,
    _host: ilmek_testing::TestHost,
    _scratch: tempfile::TempDir,
}

async fn forum() -> Forum {
    let community = Community::start().await;
    let owner = community.publisher("nuri").await;
    let (launcher, _handles) = backend_launcher(MIGRATIONS, ForumBackend::new);
    let host = start_host(&community, &owner, Arc::new(launcher)).await;
    let scratch = tempfile::tempdir().unwrap();
    let spec = PackageSpec {
        namespace: "nuri",
        slug: "forum",
        app_id: Uuid7::generate(),
        publisher_id: owner.id,
        version: Version::new(1, 0, 0),
        backend_required: true,
        permissions: vec![Permission::AppCall, Permission::AppSubscribe],
        files: vec![("index.html", b"<html></html>".to_vec())],
    };
    let package = build_package(scratch.path(), "forum", &spec, &owner.signer);
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
    let client = |member: ilmek_testing::Member| {
        AppClient::new(Arc::new(member.connector), record.clone(), record.version)
    };
    let alice = client(community.member().await);
    let bob = client(community.member().await);
    Forum {
        _community: community,
        alice,
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
async fn a_comment_from_one_member_is_visible_to_another() {
    let forum = forum().await;
    let created = forum
        .alice
        .write(
            "forum.topics.create",
            "{\"title\":\"Merhaba\",\"body\":\"İlk konu\"}".as_bytes(),
            None,
        )
        .await
        .unwrap();
    let topic_id = json(&created.body)["topic_id"].as_i64().unwrap();

    let body = format!("{{\"topic_id\":{topic_id},\"body\":\"Bob'dan yorum\"}}");
    forum
        .bob
        .write("forum.comments.add", body.as_bytes(), None)
        .await
        .unwrap();

    let view = forum
        .alice
        .read(
            "forum.topic.get",
            format!("{{\"topic_id\":{topic_id}}}").as_bytes(),
        )
        .await
        .unwrap();
    let view = json(&view.body);
    assert_eq!(view["comments"][0]["body"], "Bob'dan yorum");
    assert_ne!(
        view["comments"][0]["author"], view["topic"]["author"],
        "authorship comes from the identity"
    );

    let listed = json(
        &forum
            .bob
            .read("forum.topics.list", b"{}")
            .await
            .unwrap()
            .body,
    );
    assert_eq!(listed["topics"][0]["comment_count"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn input_is_validated_on_the_server() {
    let forum = forum().await;
    for body in [
        r#"{"title":"","body":"x"}"#.to_owned(),
        r#"{"title":"   ","body":"x"}"#.to_owned(),
        r#"{"title":"x","body":""}"#.to_owned(),
        format!(r#"{{"title":"{}","body":"x"}}"#, "ş".repeat(201)),
        r#"{"title":"x","body":"y","admin":true}"#.to_owned(),
        r#"{"title":"x"}"#.to_owned(),
    ] {
        let error = forum
            .alice
            .write("forum.topics.create", body.as_bytes(), None)
            .await
            .unwrap_err();
        assert_eq!(code(&error), Some(ErrorCode::InvalidMessage), "{body}");
    }
    let ok = format!(r#"{{"title":"{}","body":"x"}}"#, "ş".repeat(200));
    forum
        .alice
        .write("forum.topics.create", ok.as_bytes(), None)
        .await
        .unwrap();

    let missing = forum
        .alice
        .write("forum.comments.add", br#"{"topic_id":99,"body":"x"}"#, None)
        .await
        .unwrap_err();
    assert_eq!(code(&missing), Some(ErrorCode::NotFound));
}

#[tokio::test(flavor = "multi_thread")]
async fn repeating_a_write_with_its_operation_id_posts_once() {
    let forum = forum().await;
    let operation = Uuid7::generate();
    let body = r#"{"title":"Bir kez","body":"gövde"}"#.as_bytes();
    let first = forum
        .alice
        .write("forum.topics.create", body, Some(operation))
        .await
        .unwrap();
    let second = forum
        .alice
        .write("forum.topics.create", body, Some(operation))
        .await
        .unwrap();
    assert_eq!(first, second);
    let listed = json(
        &forum
            .alice
            .read("forum.topics.list", b"{}")
            .await
            .unwrap()
            .body,
    );
    assert_eq!(listed["topics"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn followers_are_told_about_new_topics() {
    let forum = forum().await;
    let mut feed = forum.bob.subscribe("forum.events", None).await.unwrap();
    forum
        .alice
        .write(
            "forum.topics.create",
            br#"{"title":"Haber","body":"yeni"}"#,
            None,
        )
        .await
        .unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), feed.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(json(&event.body)["kind"], "topic");
    feed.acknowledge(&event).await.unwrap();
}
