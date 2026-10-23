#![expect(
    clippy::unwrap_used,
    reason = "test fixtures and fakes live outside #[test] fns, where clippy.toml's allow-unwrap-in-tests does not reach"
)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use ilmek_network::{
    ApplyError, ApplyOptions, Command, Environment, NodeConfig, Plan, Probe, Runner, Step,
    TransitMode, apply, plan, rollback,
};

const NODE_A: &str = r#"
name = "NODE-A"
country = "TR"

[user_network]
interface = "wlan0"
address = "10.77.10.1/24"
ssid = "ILMEK-A"
channel = 6
passphrase_file = "/etc/ilmek/secrets/user.pass"

[user_network.dhcp]
range_start = "10.77.10.100"
range_end = "10.77.10.199"

[[user_network.dhcp.reservations]]
name = "host-1"
mac = "aa:bb:cc:dd:ee:01"
address = "10.77.10.10"

[transit]
interface = "wlan1"
address = "10.77.254.1/30"
mode = "access_point"
ssid = "ILMEK-TRANSIT"
channel = 11
passphrase_file = "/etc/ilmek/secrets/transit.pass"

[[routes]]
destination = "10.77.20.0/24"
via = "10.77.254.2"
"#;

struct FakeProbe {
    root: bool,
    management: Vec<String>,
    installed: bool,
    network_manager: bool,
    scratch: PathBuf,
}

impl FakeProbe {
    fn new(scratch: &Path) -> Self {
        Self {
            root: true,
            management: vec!["eth0".into()],
            installed: true,
            network_manager: false,
            scratch: scratch.to_owned(),
        }
    }
}

impl Probe for FakeProbe {
    fn which(&self, program: &str) -> Option<PathBuf> {
        self.installed
            .then(|| PathBuf::from(format!("/usr/sbin/{program}")))
    }
    fn is_root(&self) -> bool {
        self.root
    }
    fn exists(&self, path: &Path) -> bool {
        path == Path::new("/etc/NetworkManager") && self.network_manager
            || self.scratch.join(path.strip_prefix("/").unwrap()).exists()
    }
    fn ip_forward(&self) -> Option<String> {
        Some("0".into())
    }
    fn management_interfaces(&self) -> Vec<String> {
        self.management.clone()
    }
}

#[derive(Default)]
struct RecordingRunner {
    ran: RefCell<Vec<String>>,
    fail_on: Option<&'static str>,
}

impl Runner for RecordingRunner {
    fn run(&self, command: &Command) -> Result<String, String> {
        let text = command.to_string();
        self.ran.borrow_mut().push(text.clone());
        match self.fail_on {
            Some(needle) if text.contains(needle) => Err("simulated failure".into()),
            _ => Ok(String::new()),
        }
    }
}

struct Fixture {
    scratch: tempfile::TempDir,
    config: NodeConfig,
}

impl Fixture {
    fn new() -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let secrets = scratch.path().join("etc/ilmek/secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        std::fs::write(secrets.join("user.pass"), "user-passphrase-1\n").unwrap();
        std::fs::write(secrets.join("transit.pass"), "transit-passphrase-2\n").unwrap();
        Self {
            scratch,
            config: toml_config(NODE_A),
        }
    }

    fn root(&self) -> &Path {
        self.scratch.path()
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path.trim_start_matches('/'))).unwrap()
    }

    fn exists(&self, path: &str) -> bool {
        self.root().join(path.trim_start_matches('/')).exists()
    }
}

fn toml_config(text: &str) -> NodeConfig {
    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), text).unwrap();
    NodeConfig::load(path.path()).unwrap()
}

fn snapshots() -> &'static Path {
    Path::new("/var/lib/ilmek/network")
}

#[test]
fn the_plan_describes_every_change_and_never_contains_the_passphrase() {
    let fixture = Fixture::new();
    let probe = FakeProbe::new(fixture.root());
    let plan = plan(&fixture.config, &probe);
    let text = plan.to_string();

    assert!(plan.missing.is_empty());
    for expected in [
        "write /etc/ilmek/hostapd-wlan0.conf",
        "ssid=ILMEK-A",
        "write /etc/ilmek/hostapd-wlan1.conf",
        "dhcp-range=10.77.10.100,10.77.10.199,255.255.255.0,12h",
        "dhcp-host=aa:bb:cc:dd:ee:01,10.77.10.10,host-1",
        "ExecStart=/usr/sbin/ip route replace 10.77.20.0/24 via 10.77.254.2 dev wlan1",
        "table inet ilmek",
        "undo: /usr/sbin/sysctl -w net.ipv4.ip_forward=0",
        "@PASSPHRASE@",
    ] {
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
    }
    assert!(!text.contains("user-passphrase"), "{text}");
    // Notes explain what is NOT done; only the concrete steps must be free of NAT/default routes.
    let mut without_notes = plan.clone();
    without_notes.notes.clear();
    let steps = without_notes.to_string();
    assert!(!steps.contains("masquerade"), "{steps}");
    assert!(!steps.contains("default via"), "{steps}");
}

#[test]
fn a_station_uplink_uses_wpa_supplicant_and_no_second_access_point() {
    let fixture = Fixture::new();
    let mut config = fixture.config.clone();
    config.transit.mode = TransitMode::Station;
    config.transit.channel = None;
    let probe = FakeProbe::new(fixture.root());
    let text = plan(&config, &probe).to_string();
    assert!(text.contains("wpa-wlan1.conf"), "{text}");
    assert!(
        text.contains("-D nl80211 -c /etc/ilmek/wpa-wlan1.conf"),
        "{text}"
    );
    assert!(!text.contains("hostapd-wlan1"), "{text}");
}

#[test]
fn missing_tools_are_reported_and_block_applying() {
    let fixture = Fixture::new();
    let mut probe = FakeProbe::new(fixture.root());
    probe.installed = false;
    let plan = plan(&fixture.config, &probe);
    assert!(plan.missing.iter().any(|m| m.contains("hostapd")));

    let runner = RecordingRunner::default();
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    let error = apply(&plan, snapshots(), env, &ApplyOptions::default()).unwrap_err();
    assert!(matches!(error, ApplyError::MissingPrerequisites(_)));
    assert!(runner.ran.borrow().is_empty());
}

#[test]
fn applying_needs_root_and_leaves_the_management_interface_alone() {
    let fixture = Fixture::new();
    let runner = RecordingRunner::default();

    let mut probe = FakeProbe::new(fixture.root());
    probe.root = false;
    let built = plan(&fixture.config, &probe);
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    assert!(matches!(
        apply(&built, snapshots(), env, &ApplyOptions::default()),
        Err(ApplyError::NotRoot)
    ));

    let mut probe = FakeProbe::new(fixture.root());
    probe.management = vec!["wlan1".into()];
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    assert!(matches!(
        apply(&built, snapshots(), env, &ApplyOptions::default()),
        Err(ApplyError::ManagementInterface(name)) if name == "wlan1"
    ));
    assert!(runner.ran.borrow().is_empty());
    assert!(!fixture.exists("/etc/ilmek/hostapd-wlan0.conf"));
}

#[test]
fn a_bad_passphrase_file_is_refused_before_anything_changes() {
    let fixture = Fixture::new();
    std::fs::write(fixture.root().join("etc/ilmek/secrets/user.pass"), "short").unwrap();
    let probe = FakeProbe::new(fixture.root());
    let runner = RecordingRunner::default();
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    let built = plan(&fixture.config, &probe);
    assert!(matches!(
        apply(&built, snapshots(), env, &ApplyOptions::default()),
        Err(ApplyError::Passphrase { .. })
    ));
    assert!(!fixture.exists("/etc/ilmek/hostapd-wlan0.conf"));
    assert!(!fixture.exists("/var/lib/ilmek/network"));
}

#[test]
fn apply_writes_files_with_the_secret_and_rollback_removes_everything_it_created() {
    let fixture = Fixture::new();
    let probe = FakeProbe::new(fixture.root());
    let runner = RecordingRunner::default();
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    let built = plan(&fixture.config, &probe);

    let applied = apply(&built, snapshots(), env, &ApplyOptions::default()).unwrap();

    let hostapd = fixture.read("/etc/ilmek/hostapd-wlan0.conf");
    assert!(
        hostapd.contains("wpa_passphrase=user-passphrase-1\n"),
        "{hostapd}"
    );
    assert!(!hostapd.contains("@PASSPHRASE@"));
    assert!(
        fixture
            .read("/etc/ilmek/hostapd-wlan1.conf")
            .contains("transit-passphrase-2")
    );
    assert!(fixture.exists("/etc/systemd/system/ilmek-network.service"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &str| {
            std::fs::metadata(fixture.root().join(p.trim_start_matches('/')))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("/etc/ilmek/hostapd-wlan0.conf"), 0o600);
        assert_eq!(mode("/etc/ilmek/dnsmasq.conf"), 0o644);
        assert_eq!(
            std::fs::metadata(&applied.snapshot_dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    assert!(
        runner
            .ran
            .borrow()
            .iter()
            .any(|c| c.contains("enable --now"))
    );
    assert!(fixture.exists(&format!(
        "/var/lib/ilmek/network/{}/state/addresses.txt",
        applied.snapshot_id
    )));

    runner.ran.borrow_mut().clear();
    rollback(snapshots(), &applied.snapshot_id, env).unwrap();

    for gone in [
        "/etc/ilmek/hostapd-wlan0.conf",
        "/etc/ilmek/dnsmasq.conf",
        "/etc/systemd/system/ilmek-network.service",
        "/etc/sysctl.d/90-ilmek.conf",
    ] {
        assert!(!fixture.exists(gone), "{gone} should be gone");
    }
    assert!(
        fixture.exists("/etc/ilmek/secrets/user.pass"),
        "unrelated files stay"
    );
    let ran = runner.ran.borrow();
    let disable = ran
        .iter()
        .position(|c| c.contains("disable --now"))
        .unwrap();
    let forwarding = ran
        .iter()
        .position(|c| c.ends_with("ip_forward=0"))
        .unwrap();
    let reload = ran
        .iter()
        .rposition(|c| c.contains("daemon-reload"))
        .unwrap();
    assert!(disable < reload && forwarding < reload, "{ran:?}");

    assert!(matches!(
        rollback(snapshots(), &applied.snapshot_id, env),
        Err(ApplyError::AlreadyRolledBack(_))
    ));
}

#[test]
fn rollback_restores_the_original_content_of_files_that_existed() {
    let fixture = Fixture::new();
    let dnsmasq = fixture.root().join("etc/ilmek/dnsmasq.conf");
    std::fs::write(&dnsmasq, "# my own settings\n").unwrap();
    let probe = FakeProbe::new(fixture.root());
    let runner = RecordingRunner::default();
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    let built = plan(&fixture.config, &probe);

    let applied = apply(&built, snapshots(), env, &ApplyOptions::default()).unwrap();
    assert!(
        fixture
            .read("/etc/ilmek/dnsmasq.conf")
            .contains("dhcp-range")
    );
    rollback(snapshots(), &applied.snapshot_id, env).unwrap();
    assert_eq!(
        fixture.read("/etc/ilmek/dnsmasq.conf"),
        "# my own settings\n"
    );
}

#[test]
fn a_failure_part_way_rolls_back_automatically() {
    let fixture = Fixture::new();
    let probe = FakeProbe::new(fixture.root());
    let runner = RecordingRunner {
        fail_on: Some("enable --now"),
        ..RecordingRunner::default()
    };
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    let built = plan(&fixture.config, &probe);

    let error = apply(&built, snapshots(), env, &ApplyOptions::default()).unwrap_err();
    let ApplyError::Failed {
        cause,
        rollback_problems,
        ..
    } = error
    else {
        panic!("expected a failed apply");
    };
    assert!(cause.to_string().contains("simulated failure"));
    assert!(
        rollback_problems
            .iter()
            .all(|p| p.contains("disable --now")),
        "{rollback_problems:?}"
    );
    assert!(!fixture.exists("/etc/ilmek/hostapd-wlan0.conf"));
    assert!(!fixture.exists("/etc/systemd/system/ilmek-firewall.service"));
}

#[test]
fn rollback_rejects_unknown_and_path_like_snapshot_ids() {
    let fixture = Fixture::new();
    let probe = FakeProbe::new(fixture.root());
    let runner = RecordingRunner::default();
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    for id in ["nope", "../etc", "", ".hidden"] {
        assert!(
            matches!(
                rollback(snapshots(), id, env),
                Err(ApplyError::NoSnapshot(_))
            ),
            "{id}"
        );
    }
}

#[test]
fn network_manager_is_told_to_ignore_both_interfaces() {
    let fixture = Fixture::new();
    let mut probe = FakeProbe::new(fixture.root());
    probe.network_manager = true;
    let built: Plan = plan(&fixture.config, &probe);
    assert!(built.steps.iter().any(|s| matches!(
        s,
        Step::Write(f) if f.content == "[keyfile]\nunmanaged-devices=interface-name:wlan0;interface-name:wlan1\n"
    )));
    assert!(built.notes.iter().any(|n| n.contains("NetworkManager")));
}

#[test]
fn snapshots_report_their_phase_through_apply_and_rollback() {
    use ilmek_network::{Phase, snapshots as list_snapshots};

    let fixture = Fixture::new();
    assert!(list_snapshots(snapshots(), fixture.root()).is_empty());

    let probe = FakeProbe::new(fixture.root());
    let runner = RecordingRunner::default();
    let env = Environment {
        root: fixture.root(),
        probe: &probe,
        runner: &runner,
    };
    let built = plan(&fixture.config, &probe);
    let applied = apply(&built, snapshots(), env, &ApplyOptions::default()).unwrap();

    let listed = list_snapshots(snapshots(), fixture.root());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, applied.snapshot_id);
    assert_eq!(listed[0].node, "NODE-A");
    assert_eq!(listed[0].phase, Phase::Applied);

    rollback(snapshots(), &applied.snapshot_id, env).unwrap();
    let listed = list_snapshots(snapshots(), fixture.root());
    assert_eq!(listed[0].phase, Phase::RolledBack);
}

#[test]
fn the_shipped_example_configs_load_and_plan_without_secrets() {
    for text in [
        include_str!("../../../configs/node-a.toml.example"),
        include_str!("../../../configs/node-b.toml.example"),
    ] {
        let config = toml_config(text);
        let probe = FakeProbe::new(Path::new("/nonexistent"));
        let built = plan(&config, &probe);
        assert!(built.missing.is_empty());
        let rendered = built.to_string();
        assert!(rendered.contains("@PASSPHRASE@"), "{rendered}");
    }
}
