//! Text of the generated files. Every value interpolated here was validated by `NodeConfig`.

use std::fmt::Write as _;

use crate::config::{Band, NodeConfig, TransitMode};

/// Replaced by the contents of the passphrase file at apply time; never present in a plan.
pub(crate) const PASSPHRASE: &str = "@PASSPHRASE@";

pub(crate) const FIREWALL_TABLE: &str = "inet ilmek";

pub(crate) fn access_point(
    interface: &str,
    ssid: &str,
    channel: u16,
    band: Band,
    country: &str,
) -> String {
    let mode = match band {
        Band::Ghz2 => "g",
        Band::Ghz5 => "a",
    };
    format!(
        "interface={interface}\ndriver=nl80211\nssid={ssid}\ncountry_code={country}\n\
         hw_mode={mode}\nchannel={channel}\nieee80211n=1\nwmm_enabled=1\nauth_algs=1\nwpa=2\n\
         wpa_key_mgmt=WPA-PSK\nrsn_pairwise=CCMP\nwpa_passphrase={PASSPHRASE}\n"
    )
}

pub(crate) fn station(ssid: &str, country: &str) -> String {
    format!(
        "country={country}\nnetwork={{\n    ssid=\"{ssid}\"\n    psk=\"{PASSPHRASE}\"\n    \
         key_mgmt=WPA-PSK\n}}\n"
    )
}

/// DHCP only: DNS is switched off, and no fake internet reachability is advertised.
pub(crate) fn dhcp(config: &NodeConfig) -> String {
    let user = &config.user_network;
    let dhcp = &user.dhcp;
    let mut text = format!(
        "port=0\ninterface={}\nbind-interfaces\nno-hosts\n\
         dhcp-leasefile=/var/lib/ilmek/dnsmasq.leases\n\
         dhcp-range={},{},{},{}\ndhcp-option=option:router,{}\n",
        user.interface,
        dhcp.range_start,
        dhcp.range_end,
        user.address.netmask(),
        dhcp.lease,
        user.address.address(),
    );
    for r in &dhcp.reservations {
        let _ = writeln!(text, "dhcp-host={},{},{}", r.mac, r.address, r.name);
    }
    text
}

/// A separate table, so existing rules are left alone. The input chain never drops by policy;
/// it only keeps outsiders away from İlmek ports. Forwarding is limited to the İlmek subnets.
pub(crate) fn firewall(config: &NodeConfig) -> String {
    let subnets = join(config.firewall_subnets());
    let ports = join(config.firewall.ports.iter());
    format!(
        "add table {FIREWALL_TABLE}\ndelete table {FIREWALL_TABLE}\n\
         table {FIREWALL_TABLE} {{\n\
         \x20 set subnets {{\n    type ipv4_addr\n    flags interval\n    elements = {{ {subnets} }}\n  }}\n\
         \x20 chain input {{\n    type filter hook input priority 0; policy accept;\n\
         \x20   ip saddr != @subnets tcp dport {{ {ports} }} drop\n  }}\n\
         \x20 chain forward {{\n    type filter hook forward priority 0; policy drop;\n\
         \x20   ct state established,related accept\n\
         \x20   ip saddr @subnets ip daddr @subnets tcp dport {{ {ports} }} accept\n\
         \x20   ip saddr @subnets ip daddr @subnets icmp type {{ echo-request, echo-reply, \
         destination-unreachable, time-exceeded }} accept\n  }}\n}}\n"
    )
}

pub(crate) fn sysctl() -> &'static str {
    "net.ipv4.ip_forward = 1\n"
}

pub(crate) fn network_manager(config: &NodeConfig) -> String {
    let devices: Vec<String> = config
        .interfaces()
        .iter()
        .map(|name| format!("interface-name:{name}"))
        .collect();
    format!("[keyfile]\nunmanaged-devices={}\n", devices.join(";"))
}

pub(crate) struct Binaries<'a> {
    pub ip: &'a str,
    pub nft: &'a str,
    pub hostapd: &'a str,
    pub wpa_supplicant: &'a str,
    pub dnsmasq: &'a str,
}

pub(crate) fn hostapd_unit(interface: &str, binary: &str) -> String {
    format!(
        "[Unit]\nDescription=İlmek access point on {interface}\nAfter=network-pre.target\n\n\
         [Service]\nExecStart={binary} /etc/ilmek/hostapd-{interface}.conf\nRestart=on-failure\n\n\
         [Install]\nWantedBy=multi-user.target\n"
    )
}

pub(crate) fn station_unit(interface: &str, binary: &str) -> String {
    format!(
        "[Unit]\nDescription=İlmek transit uplink on {interface}\nAfter=network-pre.target\n\n\
         [Service]\nExecStart={binary} -i {interface} -D nl80211 -c /etc/ilmek/wpa-{interface}.conf\n\
         Restart=on-failure\n\n[Install]\nWantedBy=multi-user.target\n"
    )
}

/// Name of the unit that brings up the radio on an interface.
pub(crate) fn radio_unit(config: &NodeConfig, interface: &str) -> String {
    let station =
        config.transit.interface == interface && config.transit.mode == TransitMode::Station;
    if station {
        format!("ilmek-wpa-{interface}.service")
    } else {
        format!("ilmek-hostapd-{interface}.service")
    }
}

pub(crate) fn network_unit(config: &NodeConfig, bin: &Binaries<'_>) -> String {
    let ip = bin.ip;
    let radios: Vec<String> = config
        .interfaces()
        .iter()
        .map(|name| radio_unit(config, name))
        .collect();
    let radios = radios.join(" ");
    let mut start = String::new();
    let mut stop = String::new();
    let links = [
        (&config.user_network.interface, config.user_network.address),
        (&config.transit.interface, config.transit.address),
    ];
    for (name, address) in links {
        let _ = writeln!(start, "ExecStart={ip} link set {name} up");
        let _ = writeln!(start, "ExecStart={ip} addr replace {address} dev {name}");
        let _ = writeln!(stop, "ExecStop=-{ip} addr del {address} dev {name}");
    }
    for route in &config.routes {
        let line = format!(
            "{} via {} dev {}",
            route.destination, route.via, config.transit.interface
        );
        let _ = writeln!(start, "ExecStart={ip} route replace {line}");
        let _ = writeln!(stop, "ExecStop=-{ip} route del {line}");
    }
    format!(
        "[Unit]\nDescription=İlmek node addresses and routes\nWants={radios}\nAfter={radios}\n\
         PartOf={radios}\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nRestart=on-failure\n\
         RestartSec=3\n{start}{stop}\n[Install]\nWantedBy=multi-user.target\n"
    )
}

pub(crate) fn firewall_unit(bin: &Binaries<'_>) -> String {
    let nft = bin.nft;
    format!(
        "[Unit]\nDescription=İlmek firewall table\nAfter=network-pre.target\n\n\
         [Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart={nft} -f /etc/ilmek/firewall.nft\n\
         ExecStop=-{nft} delete table {FIREWALL_TABLE}\n\n[Install]\nWantedBy=multi-user.target\n"
    )
}

pub(crate) fn dhcp_unit(bin: &Binaries<'_>) -> String {
    let dnsmasq = bin.dnsmasq;
    format!(
        "[Unit]\nDescription=İlmek DHCP server\nRequires=ilmek-network.service\n\
         After=ilmek-network.service\n\n[Service]\nStateDirectory=ilmek\nExecStart={dnsmasq} --keep-in-foreground \
         --conf-file=/etc/ilmek/dnsmasq.conf\nRestart=on-failure\n\n\
         [Install]\nWantedBy=multi-user.target\n"
    )
}

fn join<T: ToString>(items: impl IntoIterator<Item = T>) -> String {
    items
        .into_iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
