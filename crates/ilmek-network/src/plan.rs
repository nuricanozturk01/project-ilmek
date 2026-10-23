use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::config::{NodeConfig, TransitMode};
use crate::render::{self, Binaries};
use crate::system::Probe;

const CONFIG_DIR: &str = "/etc/ilmek";
const UNIT_DIR: &str = "/etc/systemd/system";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileWrite {
    pub path: PathBuf,
    pub mode: u32,
    pub content: String,
    /// When set, [`render::PASSPHRASE`] in `content` is replaced by this file's first line.
    pub secret_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    Write(FileWrite),
    Run {
        description: String,
        command: Command,
        /// Run in reverse order on rollback.
        undo: Vec<Command>,
    },
}

/// What `apply` would do. Building it only reads the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub node: String,
    pub steps: Vec<Step>,
    /// Run once after all other rollback work, when the files are gone.
    pub after_rollback: Vec<Command>,
    pub interfaces: Vec<String>,
    /// Read-only commands whose output is saved in the snapshot for later comparison.
    pub capture: Vec<(String, Command)>,
    pub missing: Vec<String>,
    pub notes: Vec<String>,
}

struct Tools {
    paths: std::collections::HashMap<&'static str, String>,
    missing: Vec<String>,
}

impl Tools {
    fn resolve(probe: &dyn Probe, needed: &[&'static str]) -> Self {
        let mut paths = std::collections::HashMap::new();
        let mut missing = Vec::new();
        for &name in needed {
            if let Some(path) = probe.which(name) {
                paths.insert(name, path.display().to_string());
            } else {
                paths.insert(name, name.to_owned());
                missing.push(format!("`{name}` is not installed"));
            }
        }
        Self { paths, missing }
    }

    fn get(&self, name: &'static str) -> &str {
        self.paths.get(name).map_or(name, String::as_str)
    }
}

#[must_use]
pub fn build(config: &NodeConfig, probe: &dyn Probe) -> Plan {
    let station = config.transit.mode == TransitMode::Station;
    let mut needed = vec!["ip", "nft", "systemctl", "sysctl", "dnsmasq", "hostapd"];
    if station {
        needed.push("wpa_supplicant");
    }
    let tools = Tools::resolve(probe, &needed);
    let bin = Binaries {
        ip: tools.get("ip"),
        nft: tools.get("nft"),
        hostapd: tools.get("hostapd"),
        wpa_supplicant: tools.get("wpa_supplicant"),
        dnsmasq: tools.get("dnsmasq"),
    };
    let systemctl = tools.get("systemctl");
    let sysctl = tools.get("sysctl");

    let mut steps = Vec::new();
    let mut notes = Vec::new();
    let mut after_rollback = vec![Command::new(systemctl, ["daemon-reload"])];

    add_forwarding(probe, sysctl, &mut steps);
    if probe.exists(std::path::Path::new("/etc/NetworkManager")) {
        add_network_manager(config, systemctl, &mut steps, &mut after_rollback);
        notes.push(
            "NetworkManager is present: the two radios become unmanaged and lose any \
             connection NetworkManager held on them."
                .to_owned(),
        );
    }

    let mut units = Vec::new();
    add_radio_files(config, &bin, &mut steps, &mut units);
    steps.push(write(
        format!("{CONFIG_DIR}/dnsmasq.conf"),
        0o644,
        render::dhcp(config),
    ));
    steps.push(write(
        format!("{CONFIG_DIR}/firewall.nft"),
        0o644,
        render::firewall(config),
    ));
    steps.push(unit(
        "ilmek-network.service",
        render::network_unit(config, &bin),
    ));
    steps.push(unit("ilmek-firewall.service", render::firewall_unit(&bin)));
    steps.push(unit("ilmek-dnsmasq.service", render::dhcp_unit(&bin)));
    units.extend(
        [
            "ilmek-network.service",
            "ilmek-firewall.service",
            "ilmek-dnsmasq.service",
        ]
        .map(str::to_owned),
    );

    steps.push(Step::Run {
        description: "load the new unit files".into(),
        command: Command::new(systemctl, ["daemon-reload"]),
        undo: Vec::new(),
    });
    let mut enable = vec!["enable".to_owned(), "--now".to_owned()];
    enable.extend(units.iter().cloned());
    let mut disable = vec!["disable".to_owned(), "--now".to_owned()];
    disable.extend(units.iter().rev().cloned());
    steps.push(Step::Run {
        description: "start and enable the İlmek services".into(),
        command: Command::new(systemctl, enable),
        undo: vec![Command::new(systemctl, disable)],
    });

    notes.push(
        "Only the table `inet ilmek` is added; other firewall rules are untouched. Nothing is \
         masqueraded and no default route to an upstream network is created."
            .to_owned(),
    );
    Plan {
        node: config.name.clone(),
        steps,
        after_rollback,
        interfaces: config.interfaces().map(str::to_owned).to_vec(),
        capture: vec![
            ("addresses".into(), Command::new(bin.ip, ["-j", "addr"])),
            ("routes".into(), Command::new(bin.ip, ["-j", "route"])),
            (
                "firewall".into(),
                Command::new(bin.nft, ["list", "ruleset"]),
            ),
        ],
        missing: tools.missing,
        notes,
    }
}

fn add_forwarding(probe: &dyn Probe, sysctl: &str, steps: &mut Vec<Step>) {
    let previous_forwarding = probe.ip_forward().unwrap_or_else(|| "0".to_owned());
    steps.push(write(
        "/etc/sysctl.d/90-ilmek.conf",
        0o644,
        render::sysctl(),
    ));
    steps.push(Step::Run {
        description: "enable IPv4 forwarding".into(),
        command: Command::new(sysctl, ["-w", "net.ipv4.ip_forward=1"]),
        undo: vec![Command::new(
            sysctl,
            [
                "-w".to_owned(),
                format!("net.ipv4.ip_forward={previous_forwarding}"),
            ],
        )],
    });
}

fn add_network_manager(
    config: &NodeConfig,
    systemctl: &str,
    steps: &mut Vec<Step>,
    after_rollback: &mut Vec<Command>,
) {
    steps.push(write(
        "/etc/NetworkManager/conf.d/90-ilmek-unmanaged.conf",
        0o644,
        render::network_manager(config),
    ));
    let reload = Command::new(systemctl, ["reload", "NetworkManager"]);
    steps.push(Step::Run {
        description: "tell NetworkManager to leave the İlmek interfaces alone".into(),
        command: reload.clone(),
        undo: Vec::new(),
    });
    after_rollback.push(reload);
}

fn add_radio_files(
    config: &NodeConfig,
    bin: &Binaries<'_>,
    steps: &mut Vec<Step>,
    units: &mut Vec<String>,
) {
    let user = &config.user_network;
    let transit = &config.transit;
    steps.push(secret_file(
        format!("{CONFIG_DIR}/hostapd-{}.conf", user.interface),
        render::access_point(
            &user.interface,
            &user.ssid,
            user.channel,
            user.band,
            &config.country,
        ),
        &user.passphrase_file,
    ));
    steps.push(unit(
        &format!("ilmek-hostapd-{}.service", user.interface),
        render::hostapd_unit(&user.interface, bin.hostapd),
    ));
    units.push(render::radio_unit(config, &user.interface));

    match transit.mode {
        TransitMode::AccessPoint => {
            steps.push(secret_file(
                format!("{CONFIG_DIR}/hostapd-{}.conf", transit.interface),
                render::access_point(
                    &transit.interface,
                    &transit.ssid,
                    transit.channel.unwrap_or_default(),
                    transit.band,
                    &config.country,
                ),
                &transit.passphrase_file,
            ));
            steps.push(unit(
                &format!("ilmek-hostapd-{}.service", transit.interface),
                render::hostapd_unit(&transit.interface, bin.hostapd),
            ));
        }
        TransitMode::Station => {
            steps.push(secret_file(
                format!("{CONFIG_DIR}/wpa-{}.conf", transit.interface),
                render::station(&transit.ssid, &config.country),
                &transit.passphrase_file,
            ));
            steps.push(unit(
                &format!("ilmek-wpa-{}.service", transit.interface),
                render::station_unit(&transit.interface, bin.wpa_supplicant),
            ));
        }
    }
    units.push(render::radio_unit(config, &transit.interface));
}

fn write(path: impl Into<PathBuf>, mode: u32, content: impl Into<String>) -> Step {
    Step::Write(FileWrite {
        path: path.into(),
        mode,
        content: content.into(),
        secret_file: None,
    })
}

fn unit(name: &str, content: String) -> Step {
    write(format!("{UNIT_DIR}/{name}"), 0o644, content)
}

fn secret_file(path: String, content: String, secret: &std::path::Path) -> Step {
    Step::Write(FileWrite {
        path: path.into(),
        mode: 0o600,
        content,
        secret_file: Some(secret.to_owned()),
    })
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Plan for {} (nothing has been changed)\n", self.node)?;
        for (number, step) in self.steps.iter().enumerate() {
            let number = number + 1;
            match step {
                Step::Write(file) => {
                    writeln!(
                        f,
                        "{number}. write {} (mode {:o})",
                        file.path.display(),
                        file.mode
                    )?;
                    if let Some(secret) = &file.secret_file {
                        writeln!(
                            f,
                            "   passphrase read from {} at apply time",
                            secret.display()
                        )?;
                    }
                    for line in file.content.lines() {
                        writeln!(f, "   | {line}")?;
                    }
                }
                Step::Run {
                    description,
                    command,
                    undo,
                } => {
                    writeln!(f, "{number}. {description}\n   $ {command}")?;
                    for u in undo {
                        writeln!(f, "   undo: {u}")?;
                    }
                }
            }
        }
        for note in &self.notes {
            writeln!(f, "\nnote: {note}")?;
        }
        for item in &self.missing {
            writeln!(f, "\nMISSING: {item}")?;
        }
        Ok(())
    }
}
