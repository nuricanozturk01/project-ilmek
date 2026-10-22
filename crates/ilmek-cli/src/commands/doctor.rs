//! Read-only inventory of what this machine offers (README §8.3). It changes nothing.

use std::process::{Command, Stdio};

use crate::output::line;

const PROBES: &[(&str, &[&str])] = &[
    ("uname -a", &["uname", "-a"]),
    ("ip -br addr", &["ip", "-br", "addr"]),
    ("ip route", &["ip", "route"]),
    ("iw dev", &["iw", "dev"]),
    ("iw list (interface combinations)", &["iw", "list"]),
    ("rfkill list", &["rfkill", "list"]),
    ("lsusb", &["lsusb"]),
    ("lspci", &["lspci"]),
    ("nmcli general", &["nmcli", "general", "status"]),
];

fn probe(program: &str, args: &[&str]) -> String {
    match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => format!(
            "(exited with {}: {})",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(_) => format!("(`{program}` is not installed; install it to complete the inventory)"),
    }
}

pub(crate) fn run() {
    line(&format!(
        "os/arch: {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    line(&format!(
        "cpus: {}",
        std::thread::available_parallelism().map_or(0, std::num::NonZero::get)
    ));
    let memory = std::fs::read_to_string("/proc/meminfo").ok();
    if let Some(total) = memory.as_deref().and_then(|m| m.lines().next()) {
        line(&format!("memory: {}", total.trim()));
    }
    for (label, command) in PROBES {
        line(&format!("\n## {label}"));
        line(&probe(command[0], &command[1..]));
    }
    line(
        "\nNothing was changed. Wi-Fi roles (AP/STA) must be confirmed from `iw list` by a person.",
    );
}
