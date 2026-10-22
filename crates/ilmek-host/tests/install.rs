#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ilmek_core::clock::SystemClock;
use ilmek_host::{HostError, Installer, Store};
use ilmek_identity::PublisherSigner;
use ilmek_package::{init_app, pack};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::Version;
use tempfile::{TempDir, tempdir};

struct Env {
    root: TempDir,
    installer: Installer,
    store: Store,
    signer: PublisherSigner,
    publisher_id: Uuid7,
    app_id: Uuid7,
}

impl Env {
    async fn new(trust: bool) -> Self {
        let root = tempdir().unwrap();
        let data = root.path().join("host");
        fs::create_dir_all(&data).unwrap();
        let store = Store::open(&data.join("host.db")).unwrap();
        let signer = PublisherSigner::generate().unwrap();
        if trust {
            store
                .add_trusted_publisher(&signer.public_key(), 1)
                .await
                .unwrap();
        }
        let installer =
            Installer::new(data, store.clone(), 10 * 1024 * 1024, Arc::new(SystemClock));
        let publisher_id = Uuid7::generate();
        Self {
            root,
            installer,
            store,
            signer,
            publisher_id,
            app_id: Uuid7::generate(),
        }
    }

    /// Builds a signed package of `version` whose index.html says `text`.
    fn package(&self, name: &str, version: Version, text: &str) -> PathBuf {
        let source = self.root.path().join(format!("src-{name}"));
        let mut spec = init_app(
            &source,
            "nuri".parse().unwrap(),
            "forum".parse().unwrap(),
            self.publisher_id,
        )
        .unwrap();
        spec.app_id = self.app_id;
        spec.version = version;
        fs::write(source.join("app.toml"), spec.to_toml().unwrap()).unwrap();
        fs::write(source.join("assets/index.html"), text).unwrap();
        let out = self.root.path().join(format!("pkg-{name}"));
        pack(&source, &self.signer, &out).unwrap();
        out
    }
}

async fn active(env: &Env) -> Version {
    env.store
        .app(env.app_id)
        .await
        .unwrap()
        .unwrap()
        .active_version
}

#[tokio::test(flavor = "multi_thread")]
async fn install_activates_a_verified_package_and_is_idempotent() {
    let env = Env::new(true).await;
    let pkg = env.package("v1", Version::new(1, 0, 0), "<html>v1</html>");
    let first = env.installer.install(&pkg).await.unwrap();
    assert!(first.changed);
    let row = env.store.app(env.app_id).await.unwrap().unwrap();
    assert_eq!(row.active_version, Version::new(1, 0, 0));
    assert!(env.installer.data_dir_of(env.app_id).is_dir());

    assert!(matches!(
        env.installer.install(&pkg).await,
        Err(HostError::Conflict(_))
    ));
    let again = env.installer.update(&pkg).await.unwrap();
    assert!(
        !again.changed,
        "re-applying the identical package changes nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_untrusted_publisher_cannot_install() {
    let env = Env::new(false).await;
    let pkg = env.package("v1", Version::new(1, 0, 0), "<html>v1</html>");
    assert!(matches!(
        env.installer.install(&pkg).await,
        Err(HostError::UntrustedPublisher(_))
    ));
    assert!(env.store.apps().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tampered_package_is_rejected_and_leaves_nothing_behind() {
    let env = Env::new(true).await;
    let pkg = env.package("v1", Version::new(1, 0, 0), "<html>v1</html>");
    fs::write(pkg.join("assets/index.html"), "<html>evil</html>").unwrap();

    assert!(matches!(
        env.installer.install(&pkg).await,
        Err(HostError::Package(_))
    ));
    assert!(env.store.apps().await.unwrap().is_empty());
    let staging = env.root.path().join("host/staging");
    assert!(!staging.exists() || fs::read_dir(staging).unwrap().next().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_update_never_disturbs_the_active_version() {
    let env = Env::new(true).await;
    env.installer
        .install(&env.package("v1", Version::new(1, 0, 0), "<html>v1</html>"))
        .await
        .unwrap();

    let tampered = env.package("v2", Version::new(2, 0, 0), "<html>v2</html>");
    fs::write(tampered.join("assets/index.html"), "<html>tampered</html>").unwrap();
    assert!(env.installer.update(&tampered).await.is_err());
    assert_eq!(active(&env).await, Version::new(1, 0, 0));

    let stranger = PublisherSigner::generate().unwrap();
    let foreign_src = env.root.path().join("foreign-src");
    let mut spec = init_app(
        &foreign_src,
        "nuri".parse().unwrap(),
        "forum".parse().unwrap(),
        env.publisher_id,
    )
    .unwrap();
    spec.app_id = env.app_id;
    spec.version = Version::new(3, 0, 0);
    fs::write(foreign_src.join("app.toml"), spec.to_toml().unwrap()).unwrap();
    let foreign = env.root.path().join("foreign");
    pack(&foreign_src, &stranger, &foreign).unwrap();
    assert!(matches!(
        env.installer.update(&foreign).await,
        Err(HostError::UntrustedPublisher(_))
    ));
    assert_eq!(active(&env).await, Version::new(1, 0, 0));

    env.installer
        .update(&env.package("v2ok", Version::new(2, 0, 0), "<html>v2</html>"))
        .await
        .unwrap();
    assert_eq!(active(&env).await, Version::new(2, 0, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn updates_must_move_forward_keep_identity_and_rollback_is_explicit() {
    let env = Env::new(true).await;
    env.installer
        .install(&env.package("v2", Version::new(2, 0, 0), "<html>v2</html>"))
        .await
        .unwrap();

    let older = env.package("v1", Version::new(1, 0, 0), "<html>v1</html>");
    assert!(matches!(
        env.installer.update(&older).await,
        Err(HostError::Conflict(_))
    ));

    let same_version_other_content =
        env.package("v2b", Version::new(2, 0, 0), "<html>different</html>");
    assert!(matches!(
        env.installer.update(&same_version_other_content).await,
        Err(HostError::Conflict(_))
    ));

    env.installer
        .update(&env.package("v3", Version::new(3, 0, 0), "<html>v3</html>"))
        .await
        .unwrap();
    env.installer
        .rollback(env.app_id, Version::new(2, 0, 0))
        .await
        .unwrap();
    assert_eq!(active(&env).await, Version::new(2, 0, 0));
    assert!(matches!(
        env.installer
            .rollback(env.app_id, Version::new(9, 9, 9))
            .await,
        Err(HostError::UnknownVersion)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn removal_keeps_data_unless_purge_is_explicit() {
    let env = Env::new(true).await;
    env.installer
        .install(&env.package("v1", Version::new(1, 0, 0), "<html>v1</html>"))
        .await
        .unwrap();
    let data = env.installer.data_dir_of(env.app_id);
    fs::write(data.join("precious.db"), "data").unwrap();

    env.store
        .set_desired(env.app_id, ilmek_host::Desired::Running, false)
        .await
        .unwrap();
    assert!(matches!(
        env.installer.remove(env.app_id, false).await,
        Err(HostError::Conflict(_))
    ));
    env.store
        .set_desired(env.app_id, ilmek_host::Desired::Stopped, false)
        .await
        .unwrap();

    env.installer.remove(env.app_id, false).await.unwrap();
    assert!(env.store.app(env.app_id).await.unwrap().is_none());
    assert!(
        data.join("precious.db").exists(),
        "removal must not delete user data"
    );

    env.installer
        .install(&env.package("again", Version::new(1, 0, 0), "<html>v1</html>"))
        .await
        .unwrap();
    env.store
        .set_desired(env.app_id, ilmek_host::Desired::Stopped, false)
        .await
        .unwrap();
    env.installer.remove(env.app_id, true).await.unwrap();
    assert!(!data.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn start_up_cleanup_removes_staging_and_orphan_version_directories() {
    let env = Env::new(true).await;
    env.installer
        .install(&env.package("v1", Version::new(1, 0, 0), "<html>v1</html>"))
        .await
        .unwrap();
    let staging = env.root.path().join("host/staging/half-copied");
    fs::create_dir_all(&staging).unwrap();
    let orphan = env
        .installer
        .app_dir(env.app_id)
        .join("versions/9.9.9-deadbeef0000");
    fs::create_dir_all(&orphan).unwrap();

    env.installer.clean_leftovers().await.unwrap();
    assert!(!staging.exists() && !orphan.exists());
    let versions = env.store.versions(env.app_id).await.unwrap();
    assert!(
        Path::new(&versions[0].directory).is_dir(),
        "the installed version survives cleanup"
    );
}
