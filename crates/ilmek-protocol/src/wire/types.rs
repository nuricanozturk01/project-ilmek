//! Validated scalar types (PROTOCOL.md §4, §15).

use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid {kind}: {reason}")]
pub struct InvalidValue {
    kind: &'static str,
    reason: &'static str,
}

fn invalid(kind: &'static str, reason: &'static str) -> InvalidValue {
    InvalidValue { kind, reason }
}

macro_rules! string_serde {
    ($ty:ty) => {
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Namespace or application slug: `[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(String);

impl FromStr for Name {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        let bytes = text.as_bytes();
        let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        let well_formed = (1..=48).contains(&bytes.len())
            && bytes.iter().all(|&b| alnum(b) || b == b'-')
            && alnum(bytes[0])
            && alnum(bytes[bytes.len() - 1]);
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(invalid("name", "grammar"))
        }
    }
}

impl Name {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
string_serde!(Name);

/// RPC method or topic: `[a-z][a-z0-9_.]{0,63}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Method(String);

impl FromStr for Method {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        let bytes = text.as_bytes();
        let well_formed = (1..=64).contains(&bytes.len())
            && bytes[0].is_ascii_lowercase()
            && bytes
                .iter()
                .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.');
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(invalid("method", "grammar"))
        }
    }
}

impl Method {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
string_serde!(Method);

/// `major.minor.patch`, no leading zeros, each component in 0..=2^31-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl FromStr for Version {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        let mut parts = text.split('.').map(|part| {
            let canonical = !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'));
            canonical
                .then(|| {
                    part.parse::<u32>()
                        .ok()
                        .filter(|&n| i32::try_from(n).is_ok())
                })
                .flatten()
        });
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) => Ok(Self {
                major,
                minor,
                patch,
            }),
            _ => Err(invalid("version", "expected major.minor.patch")),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
string_serde!(Version);

/// SHA-256 digest, 64 lowercase hex characters on the wire.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash([u8; 32]);

impl Hash {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl FromStr for Hash {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        let bytes = text.as_bytes();
        if bytes.len() != 64 {
            return Err(invalid("hash", "expected 64 hex characters"));
        }
        let nibble = |b: u8| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            _ => None,
        };
        let mut out = [0u8; 32];
        for (slot, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
            match (nibble(pair[0]), nibble(pair[1])) {
                (Some(high), Some(low)) => *slot = high << 4 | low,
                _ => return Err(invalid("hash", "lowercase hex only")),
            }
        }
        Ok(Self(out))
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({self})")
    }
}
string_serde!(Hash);

/// Binary value carried as unpadded base64url.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Token(Vec<u8>);

impl Token {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl FromStr for Token {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        URL_SAFE_NO_PAD
            .decode(text)
            .map(Self)
            .map_err(|_| invalid("token", "not unpadded base64url"))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&URL_SAFE_NO_PAD.encode(&self.0))
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({} bytes)", self.0.len())
    }
}
string_serde!(Token);

/// Opaque server-issued position, at most 512 base64url characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cursor(String);

impl Cursor {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Cursor {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        let valid = (1..=512).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(invalid("cursor", "grammar"))
        }
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
string_serde!(Cursor);

/// Free text bounded to `MAX` UTF-8 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Text<const MAX: usize>(String);

impl<const MAX: usize> Text<MAX> {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> FromStr for Text<MAX> {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        if text.len() <= MAX {
            Ok(Self(text.to_owned()))
        } else {
            Err(invalid("text", "too long"))
        }
    }
}

impl<const MAX: usize> fmt::Display for Text<MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<const MAX: usize> Serialize for Text<MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for Text<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Canonical IPv4 address and port 1..=65535.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawEndpoint", into = "RawEndpoint")]
pub struct Endpoint {
    pub ip: Ipv4Addr,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEndpoint {
    ip: String,
    port: u64,
}

impl TryFrom<RawEndpoint> for Endpoint {
    type Error = InvalidValue;

    fn try_from(raw: RawEndpoint) -> Result<Self, InvalidValue> {
        let ip: Ipv4Addr = raw.ip.parse().map_err(|_| invalid("endpoint", "ip"))?;
        if ip.to_string() != raw.ip {
            return Err(invalid("endpoint", "ip not canonical"));
        }
        let port = u16::try_from(raw.port)
            .ok()
            .filter(|&port| port != 0)
            .ok_or_else(|| invalid("endpoint", "port"))?;
        Ok(Self { ip, port })
    }
}

impl From<Endpoint> for RawEndpoint {
    fn from(endpoint: Endpoint) -> Self {
        Self {
            ip: endpoint.ip.to_string(),
            port: u64::from(endpoint.port),
        }
    }
}

/// Package-relative POSIX path (PROTOCOL.md §15).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackagePath(String);

impl PackagePath {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const WINDOWS_DEVICES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn valid_segment(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.ends_with('.')
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !WINDOWS_DEVICES.contains(&stem.as_str())
}

impl FromStr for PackagePath {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        if text.len() <= 240 && text.split('/').all(valid_segment) {
            Ok(Self(text.to_owned()))
        } else {
            Err(invalid("path", "grammar"))
        }
    }
}

impl fmt::Display for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
string_serde!(PackagePath);
