use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cidr::Cidr;
use crate::error::ConfigError;

/// `node.toml`: everything a Linux routing node needs. No address is built into the program.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    /// Label used in snapshots and reports, e.g. `NODE-A`.
    pub name: String,
    /// Two-letter regulatory domain; radios must not transmit without one.
    pub country: String,
    #[serde(default = "default_snapshot_dir")]
    pub snapshot_dir: PathBuf,
    pub user_network: UserNetwork,
    pub transit: Transit,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub firewall: Firewall,
}

fn default_snapshot_dir() -> PathBuf {
    PathBuf::from("/var/lib/ilmek/network")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserNetwork {
    pub interface: String,
    pub address: Cidr,
    pub ssid: String,
    pub channel: u16,
    #[serde(default)]
    pub band: Band,
    /// File with the Wi-Fi passphrase (8–63 printable ASCII characters), readable by root only.
    pub passphrase_file: PathBuf,
    pub dhcp: Dhcp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dhcp {
    pub range_start: Ipv4Addr,
    pub range_end: Ipv4Addr,
    #[serde(default = "default_lease")]
    pub lease: String,
    #[serde(default)]
    pub reservations: Vec<Reservation>,
}

fn default_lease() -> String {
    "12h".to_owned()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub name: String,
    pub mac: String,
    pub address: Ipv4Addr,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transit {
    pub interface: String,
    pub address: Cidr,
    pub mode: TransitMode,
    pub ssid: String,
    /// Required for `access_point`; a station follows the access point's channel.
    pub channel: Option<u16>,
    #[serde(default)]
    pub band: Band,
    pub passphrase_file: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitMode {
    AccessPoint,
    Station,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Band {
    #[default]
    #[serde(rename = "2.4")]
    Ghz2,
    #[serde(rename = "5")]
    Ghz5,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub destination: Cidr,
    pub via: Ipv4Addr,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Firewall {
    /// Subnets that may use İlmek ports and be forwarded between. Defaults to the node's own
    /// two networks plus every route destination.
    #[serde(default)]
    pub subnets: Vec<Cidr>,
    /// TCP ports forwarded between the subnets and protected from outsiders.
    #[serde(default = "default_ports")]
    pub ports: Vec<u16>,
}

impl Default for Firewall {
    fn default() -> Self {
        Self {
            subnets: Vec::new(),
            ports: default_ports(),
        }
    }
}

fn default_ports() -> Vec<u16> {
    vec![47401, 47402]
}

impl NodeConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let config: Self =
            toml::from_str(&text).map_err(|e| ConfigError::Invalid(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// The subnets the firewall treats as İlmek territory.
    #[must_use]
    pub fn firewall_subnets(&self) -> Vec<Cidr> {
        let mut subnets = self.firewall.subnets.clone();
        if subnets.is_empty() {
            subnets.push(self.user_network.address.network());
            subnets.push(self.transit.address.network());
            subnets.extend(self.routes.iter().map(|r| r.destination.network()));
        }
        subnets.dedup();
        subnets
    }

    /// Interfaces this configuration takes over.
    #[must_use]
    pub fn interfaces(&self) -> [&str; 2] {
        [&self.user_network.interface, &self.transit.interface]
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let user = &self.user_network;
        let transit = &self.transit;

        label("name", &self.name)?;
        if self.country.len() != 2 || !self.country.bytes().all(|b| b.is_ascii_uppercase()) {
            return invalid("country must be a two-letter upper-case regulatory domain, e.g. TR");
        }
        interface_name(&user.interface)?;
        interface_name(&transit.interface)?;
        if user.interface == transit.interface {
            return invalid("user_network and transit must use different interfaces");
        }
        ssid(&user.ssid)?;
        ssid(&transit.ssid)?;
        channel(user.channel, user.band)?;
        if let Some(value) = transit.channel {
            channel(value, transit.band)?;
        }
        if transit.mode == TransitMode::AccessPoint && transit.channel.is_none() {
            return invalid("transit.channel is required when transit.mode is access_point");
        }
        if user.address.network().overlaps(transit.address.network()) {
            return invalid("user_network.address and transit.address overlap");
        }
        if transit.address.prefix() > 30 {
            return invalid("transit.address needs room for two nodes (prefix 30 or shorter)");
        }
        self.validate_dhcp()?;
        self.validate_routes()?;
        if self.firewall.ports.is_empty() {
            return invalid("firewall.ports must not be empty");
        }
        Ok(())
    }

    fn validate_dhcp(&self) -> Result<(), ConfigError> {
        let user = &self.user_network;
        let dhcp = &user.dhcp;
        let inside = |ip| user.address.contains(ip);
        if !inside(dhcp.range_start) || !inside(dhcp.range_end) {
            return invalid("dhcp range must lie inside user_network.address");
        }
        if u32::from(dhcp.range_start) > u32::from(dhcp.range_end) {
            return invalid("dhcp.range_start is above dhcp.range_end");
        }
        let range = u32::from(dhcp.range_start)..=u32::from(dhcp.range_end);
        if range.contains(&u32::from(user.address.address())) {
            return invalid("the node's own address lies inside the dhcp range");
        }
        let valid_lease = dhcp.lease.len() <= 8
            && dhcp.lease.ends_with(['s', 'm', 'h', 'd'])
            && dhcp.lease[..dhcp.lease.len() - 1]
                .bytes()
                .all(|b| b.is_ascii_digit())
            && dhcp.lease.len() > 1;
        if !valid_lease {
            return invalid("dhcp.lease must look like 30m, 12h or 2d");
        }
        for reservation in &dhcp.reservations {
            label("reservation name", &reservation.name)?;
            if !valid_mac(&reservation.mac) {
                return invalid(format!("`{}` is not a MAC address", reservation.mac));
            }
            if !inside(reservation.address) || range.contains(&u32::from(reservation.address)) {
                return invalid(format!(
                    "reservation {} must be inside the network but outside the dynamic range",
                    reservation.name
                ));
            }
        }
        Ok(())
    }

    fn validate_routes(&self) -> Result<(), ConfigError> {
        for route in &self.routes {
            if !self.transit.address.contains(route.via) {
                return invalid(format!(
                    "route via {} is not inside the transit network",
                    route.via
                ));
            }
            if route.via == self.transit.address.address() {
                return invalid("a route cannot use this node as its own next hop");
            }
            let destination = route.destination.network();
            if destination.overlaps(self.user_network.address.network())
                || destination.overlaps(self.transit.address.network())
            {
                return invalid(format!(
                    "route {} overlaps a directly connected network",
                    route.destination
                ));
            }
        }
        Ok(())
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid(message.into()))
}

/// Names end up in generated files, so they are restricted to a safe alphabet.
fn label(what: &str, value: &str) -> Result<(), ConfigError> {
    let fine = !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if fine {
        Ok(())
    } else {
        invalid(format!(
            "{what} must be 1–32 characters of letters, digits, '-', '_' or '.'"
        ))
    }
}

fn interface_name(value: &str) -> Result<(), ConfigError> {
    let fine = !value.is_empty()
        && value.len() <= 15
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if fine {
        Ok(())
    } else {
        invalid(format!("`{value}` is not a valid interface name"))
    }
}

fn ssid(value: &str) -> Result<(), ConfigError> {
    let fine = !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\');
    if fine {
        Ok(())
    } else {
        invalid("an SSID is 1–32 printable ASCII characters without quotes or backslashes")
    }
}

fn channel(value: u16, band: Band) -> Result<(), ConfigError> {
    let fine = match band {
        Band::Ghz2 => (1..=13).contains(&value),
        Band::Ghz5 => matches!(value, 36..=64 | 100..=144 | 149..=165),
    };
    if fine {
        Ok(())
    } else {
        invalid(format!("channel {value} does not exist in that band"))
    }
}

fn valid_mac(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const NODE_A: &str = r#"
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

    pub(crate) fn node_a() -> NodeConfig {
        let config: NodeConfig = toml::from_str(NODE_A).unwrap();
        config.validate().unwrap();
        config
    }

    fn rejects(edit: impl FnOnce(&mut NodeConfig), expected: &str) {
        let mut config = node_a();
        edit(&mut config);
        let message = config.validate().unwrap_err().to_string();
        assert!(message.contains(expected), "{message}");
    }

    #[test]
    fn the_reference_configuration_is_valid_with_derived_firewall_subnets() {
        let subnets: Vec<String> = node_a()
            .firewall_subnets()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            subnets,
            ["10.77.10.0/24", "10.77.254.0/30", "10.77.20.0/24"]
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = format!("{NODE_A}\nsurprise = true");
        assert!(toml::from_str::<NodeConfig>(&text).is_err());
    }

    #[test]
    fn unsafe_or_inconsistent_values_are_rejected() {
        rejects(|c| c.country = "tr".into(), "regulatory domain");
        rejects(|c| c.user_network.ssid = "a\"b".into(), "SSID");
        rejects(
            |c| c.user_network.interface = "wlan0;rm".into(),
            "interface name",
        );
        rejects(
            |c| c.transit.interface = "wlan0".into(),
            "different interfaces",
        );
        rejects(|c| c.user_network.channel = 14, "channel");
        rejects(|c| c.transit.channel = None, "transit.channel");
        rejects(
            |c| c.transit.address = "10.77.10.9/30".parse().unwrap(),
            "overlap",
        );
        rejects(
            |c| c.user_network.dhcp.range_start = "10.77.11.5".parse().unwrap(),
            "inside user_network",
        );
        rejects(
            |c| c.user_network.dhcp.range_end = "10.77.10.50".parse().unwrap(),
            "above",
        );
        rejects(
            |c| c.user_network.dhcp.reservations[0].mac = "zz".into(),
            "MAC",
        );
        rejects(
            |c| c.user_network.dhcp.reservations[0].address = "10.77.10.150".parse().unwrap(),
            "outside the dynamic range",
        );
        rejects(|c| c.user_network.dhcp.lease = "forever".into(), "lease");
        rejects(
            |c| c.routes[0].via = "10.77.99.2".parse().unwrap(),
            "transit network",
        );
        rejects(
            |c| c.routes[0].destination = "10.77.10.0/24".parse().unwrap(),
            "directly connected",
        );
        rejects(|c| c.firewall.ports.clear(), "ports");
    }
}
