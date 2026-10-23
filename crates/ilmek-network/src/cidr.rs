use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// An IPv4 address with its prefix length, e.g. `10.77.10.1/24`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Cidr {
    address: Ipv4Addr,
    prefix: u8,
}

impl Cidr {
    #[must_use]
    pub fn address(self) -> Ipv4Addr {
        self.address
    }

    #[must_use]
    pub fn prefix(self) -> u8 {
        self.prefix
    }

    fn mask(self) -> u32 {
        u32::MAX
            .checked_shl(32 - u32::from(self.prefix))
            .unwrap_or(0)
    }

    #[must_use]
    pub fn netmask(self) -> Ipv4Addr {
        Ipv4Addr::from(self.mask())
    }

    /// The same prefix with host bits cleared.
    #[must_use]
    pub fn network(self) -> Self {
        Self {
            address: Ipv4Addr::from(u32::from(self.address) & self.mask()),
            prefix: self.prefix,
        }
    }

    #[must_use]
    pub fn contains(self, other: Ipv4Addr) -> bool {
        u32::from(other) & self.mask() == u32::from(self.address) & self.mask()
    }

    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.contains(other.network().address) || other.contains(self.network().address)
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

impl FromStr for Cidr {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = text
            .split_once('/')
            .ok_or_else(|| format!("`{text}` is not in address/prefix form"))?;
        let address = address
            .parse()
            .map_err(|_| format!("`{address}` is not an IPv4 address"))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|_| format!("`{prefix}` is not a prefix length"))?;
        if prefix > 32 {
            return Err(format!("prefix length {prefix} is above 32"));
        }
        Ok(Self { address, prefix })
    }
}

impl TryFrom<String> for Cidr {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Cidr> for String {
    fn from(cidr: Cidr) -> Self {
        cidr.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_masks() {
        let cidr: Cidr = "10.77.10.1/24".parse().unwrap();
        assert_eq!(cidr.network().to_string(), "10.77.10.0/24");
        assert_eq!(cidr.netmask(), Ipv4Addr::new(255, 255, 255, 0));
        assert!(cidr.contains(Ipv4Addr::new(10, 77, 10, 200)));
        assert!(!cidr.contains(Ipv4Addr::new(10, 77, 11, 1)));
    }

    #[test]
    fn zero_prefix_matches_everything_and_bad_input_is_rejected() {
        let all: Cidr = "0.0.0.0/0".parse().unwrap();
        assert!(all.contains(Ipv4Addr::new(8, 8, 8, 8)));
        for bad in ["10.0.0.1", "10.0.0.1/33", "x/24", "10.0.0.1/x"] {
            assert!(bad.parse::<Cidr>().is_err(), "{bad}");
        }
    }

    #[test]
    fn detects_overlap() {
        let a: Cidr = "10.77.10.0/24".parse().unwrap();
        let wide: Cidr = "10.77.0.0/16".parse().unwrap();
        let b: Cidr = "10.77.20.0/24".parse().unwrap();
        assert!(a.overlaps(wide) && wide.overlaps(a));
        assert!(!a.overlaps(b));
    }
}
