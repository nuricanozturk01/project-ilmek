//! The first-demo scenario with the real `ilmek` binary and real sockets: one machine runs the
//! node and publishes an app, a second device (its own home directory, as if it were another
//! computer) finds the node by itself, joins and downloads the app. Everything is on loopback.

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test helpers live outside #[test] fns"
)]

use std::net::{Ipv4Addr, TcpStream, UdpSocket};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const HELLO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello-html/app");

struct Node(Child);

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_tcp() -> u16 {
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn free_udp() -> u16 {
    UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn ilmek(home: &Path, discovery: u16, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ilmek"))
        .args(args)
        .env("ILMEK_HOME", home)
        .env("ILMEK_DISCOVERY_PORT", discovery.to_string())
        .output()
        .expect("the ilmek binary starts")
}

fn ok(home: &Path, discovery: u16, args: &[&str]) -> String {
    let out = ilmek(home, discovery, args);
    assert!(
        out.status.success(),
        "ilmek {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn a_second_device_finds_the_node_joins_and_opens_a_published_app() {
    let node_home = tempfile::tempdir().unwrap();
    let client_home = tempfile::tempdir().unwrap();
    let (registry_port, host_port, discovery) = (free_tcp(), free_tcp(), free_udp());

    let node = Node(
        Command::new(env!("CARGO_BIN_EXE_ilmek"))
            .args([
                "node",
                "--registry-port",
                &registry_port.to_string(),
                "--host-port",
                &host_port.to_string(),
            ])
            .env("ILMEK_HOME", node_home.path())
            .env("ILMEK_DISCOVERY_PORT", discovery.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the node starts"),
    );
    wait_for("the registry port", || {
        TcpStream::connect((Ipv4Addr::LOCALHOST, registry_port)).is_ok()
    });
    wait_for("the host port", || {
        TcpStream::connect((Ipv4Addr::LOCALHOST, host_port)).is_ok()
    });

    // On the node: pack, install, start and publish. The publisher device joins by itself.
    let package = node_home.path().join("pkg");
    let package_arg = package.to_str().unwrap();
    ok(
        node_home.path(),
        discovery,
        &["app", "pack", HELLO, "--out", package_arg],
    );
    ok(
        node_home.path(),
        discovery,
        &["app", "install", package_arg],
    );
    ok(
        node_home.path(),
        discovery,
        &["app", "start", "nuri/merhaba"],
    );
    wait_for("the app to run", || {
        ok(node_home.path(), discovery, &["app", "list"]).contains("state=running")
    });
    wait_for("the host to register", || {
        ilmek(
            node_home.path(),
            discovery,
            &["app", "publish", "nuri/merhaba"],
        )
        .status
        .success()
    });

    // A different "computer": no files, no address. It has to find the node itself.
    let joined = ok(client_home.path(), discovery, &["join"]);
    assert!(joined.starts_with("device "), "{joined}");
    let who = ok(client_home.path(), discovery, &["whoami"]);
    assert!(who.contains(&format!("127.0.0.1:{registry_port}")), "{who}");

    let resolved = ok(
        client_home.path(),
        discovery,
        &["resolve", "ilmek://nuri/merhaba"],
    );
    assert!(resolved.contains("RecentHeartbeat"), "{resolved}");

    let cache = client_home.path().join("downloaded");
    let fetched = ok(
        client_home.path(),
        discovery,
        &[
            "fetch",
            "ilmek://nuri/merhaba",
            "--cache",
            cache.to_str().unwrap(),
        ],
    );
    let folder = Path::new(fetched.trim());
    assert!(folder.join("assets/index.html").is_file(), "{fetched}");

    // A device that joins without discovery, only by address, ends up in the same community.
    let other_home = tempfile::tempdir().unwrap();
    let by_address = ok(
        other_home.path(),
        discovery,
        &["join", "--registry", &format!("127.0.0.1:{registry_port}")],
    );
    assert!(by_address.starts_with("device "));
    drop(node);
}

#[test]
fn without_a_node_the_error_says_what_to_do() {
    let home = tempfile::tempdir().unwrap();
    let out = ilmek(
        home.path(),
        free_udp(),
        &["resolve", "ilmek://nuri/merhaba"],
    );
    assert!(!out.status.success());
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(message.contains("ilmek node"), "{message}");
}

#[test]
fn a_changed_port_in_the_configuration_is_what_devices_are_told() {
    let home = tempfile::tempdir().unwrap();
    let client = tempfile::tempdir().unwrap();
    let discovery = free_udp();
    let (first, second, host_port) = (free_tcp(), free_tcp(), free_tcp());

    let spawn_node = |registry_port: u16| {
        Node(
            Command::new(env!("CARGO_BIN_EXE_ilmek"))
                .args([
                    "node",
                    "--registry-port",
                    &registry_port.to_string(),
                    "--host-port",
                    &host_port.to_string(),
                ])
                .env("ILMEK_HOME", home.path())
                .env("ILMEK_DISCOVERY_PORT", discovery.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("the node starts"),
        )
    };

    let node = spawn_node(first);
    wait_for("the first registry port", || {
        TcpStream::connect((Ipv4Addr::LOCALHOST, first)).is_ok()
    });
    drop(node);

    // The operator edits the port in registry.toml; the stored community profile still has the old one.
    let config = home.path().join("registry.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        text.replace(&first.to_string(), &second.to_string()),
    )
    .unwrap();

    let _node = spawn_node(second);
    wait_for("the new registry port", || {
        TcpStream::connect((Ipv4Addr::LOCALHOST, second)).is_ok()
    });
    wait_for("the host to register under the new port", || {
        ilmek(client.path(), discovery, &["join"]).status.success()
    });
    let who = ok(client.path(), discovery, &["whoami"]);
    assert!(who.contains(&format!("127.0.0.1:{second}")), "{who}");
}

#[test]
fn an_extra_host_finds_the_node_registers_itself_and_serves_its_own_app() {
    let node_home = tempfile::tempdir().unwrap();
    let host_home = tempfile::tempdir().unwrap();
    let client_home = tempfile::tempdir().unwrap();
    let (registry_port, node_host_port, extra_port, discovery) =
        (free_tcp(), free_tcp(), free_tcp(), free_udp());

    let _node = Node(
        Command::new(env!("CARGO_BIN_EXE_ilmek"))
            .args([
                "node",
                "--registry-port",
                &registry_port.to_string(),
                "--host-port",
                &node_host_port.to_string(),
            ])
            .env("ILMEK_HOME", node_home.path())
            .env("ILMEK_DISCOVERY_PORT", discovery.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the node starts"),
    );
    wait_for("the registry port", || {
        TcpStream::connect((Ipv4Addr::LOCALHOST, registry_port)).is_ok()
    });

    // Another "machine": nothing is configured, it has to find the node and join by itself.
    let _extra = Node(
        Command::new(env!("CARGO_BIN_EXE_ilmek"))
            .args(["host", "start", "--port", &extra_port.to_string()])
            .env("ILMEK_HOME", host_home.path())
            .env("ILMEK_DISCOVERY_PORT", discovery.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the host starts"),
    );
    wait_for("the extra host's port", || {
        TcpStream::connect((Ipv4Addr::LOCALHOST, extra_port)).is_ok()
    });

    // It serves its own application, installed with the ordinary commands on that machine.
    let package = host_home.path().join("pkg");
    let package_arg = package.to_str().unwrap();
    let home = host_home.path();
    ok(
        home,
        discovery,
        &["app", "pack", HELLO, "--out", package_arg],
    );
    ok(home, discovery, &["app", "install", package_arg]);
    ok(home, discovery, &["app", "start", "nuri/merhaba"]);
    wait_for("the app to run", || {
        ok(home, discovery, &["app", "list"]).contains("state=running")
    });
    ok(home, discovery, &["app", "publish", "nuri/merhaba"]);

    let resolved = ok(
        client_home.path(),
        discovery,
        &["resolve", "ilmek://nuri/merhaba"],
    );
    assert!(
        resolved.contains(&format!(":{extra_port}")),
        "the record must point at the extra host: {resolved}"
    );
    let cache = client_home.path().join("downloaded");
    let fetched = ok(
        client_home.path(),
        discovery,
        &[
            "fetch",
            "ilmek://nuri/merhaba",
            "--cache",
            cache.to_str().unwrap(),
        ],
    );
    assert!(
        Path::new(fetched.trim())
            .join("assets/index.html")
            .is_file(),
        "{fetched}"
    );
}
