//! The two seams to the operating system, so planning and applying can be tested without
//! touching one.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::command::Command;

/// Read-only questions about the machine.
pub trait Probe {
    fn which(&self, program: &str) -> Option<PathBuf>;
    fn is_root(&self) -> bool;
    fn exists(&self, path: &Path) -> bool;
    /// Current value of `net.ipv4.ip_forward`.
    fn ip_forward(&self) -> Option<String>;
    /// Interfaces that carry the default route or the current remote login session.
    fn management_interfaces(&self) -> Vec<String>;
}

/// Executes commands. Returns standard output, or a reason for failure.
pub trait Runner {
    fn run(&self, command: &Command) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct LinuxProbe;

const EXTRA_PATH: [&str; 3] = ["/usr/sbin", "/sbin", "/usr/local/sbin"];

impl Probe for LinuxProbe {
    fn which(&self, program: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .chain(EXTRA_PATH.iter().map(PathBuf::from))
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    }

    fn is_root(&self) -> bool {
        capture("id", &["-u"]).is_some_and(|out| out.trim() == "0")
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn ip_forward(&self) -> Option<String> {
        std::fs::read_to_string("/proc/sys/net/ipv4/ip_forward")
            .ok()
            .map(|v| v.trim().to_owned())
    }

    fn management_interfaces(&self) -> Vec<String> {
        let mut found = Vec::new();
        if let Some(routes) = capture("ip", &["-j", "route", "show", "default"]) {
            found.extend(json_strings(&routes, "dev"));
        }
        if let Some(ssh) = std::env::var("SSH_CONNECTION")
            .ok()
            .and_then(|value| value.split_whitespace().nth(2).map(str::to_owned))
            && let Some(addresses) = capture("ip", &["-j", "addr"])
        {
            found.extend(interfaces_owning(&addresses, &ssh));
        }
        found.sort();
        found.dedup();
        found
    }
}

fn capture(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn json_strings(json: &str, key: &str) -> Vec<String> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| entry.get(key)?.as_str().map(str::to_owned))
        .collect()
}

fn interfaces_owning(addr_json: &str, ip: &str) -> Vec<String> {
    serde_json::from_str::<Vec<serde_json::Value>>(addr_json)
        .unwrap_or_default()
        .iter()
        .filter(|iface| {
            iface["addr_info"]
                .as_array()
                .is_some_and(|infos| infos.iter().any(|info| info["local"].as_str() == Some(ip)))
        })
        .filter_map(|iface| iface["ifname"].as_str().map(str::to_owned))
        .collect()
}

#[derive(Debug, Default)]
pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, command: &Command) -> Result<String, String> {
        let output = std::process::Command::new(&command.program)
            .args(&command.args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot start: {e}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(format!(
                "{}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_default_route_and_ssh_interfaces_in_ip_json() {
        let routes = r#"[{"dst":"default","gateway":"192.168.1.1","dev":"eth0"}]"#;
        assert_eq!(json_strings(routes, "dev"), ["eth0"]);
        let addresses = r#"[{"ifname":"lo","addr_info":[{"local":"127.0.0.1"}]},
            {"ifname":"wlan0","addr_info":[{"local":"10.0.0.5"},{"local":"fe80::1"}]}]"#;
        assert_eq!(interfaces_owning(addresses, "10.0.0.5"), ["wlan0"]);
        assert!(interfaces_owning(addresses, "10.0.0.6").is_empty());
        assert!(json_strings("not json", "dev").is_empty());
    }
}
