//! `UUIDv7` identifiers (PROTOCOL.md §4.1).

use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use uuid::{Uuid, Variant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
#[error("not a canonical lowercase RFC 9562 UUIDv7")]
pub struct InvalidUuid;

/// A UUID that is guaranteed to be a canonical, lowercase, RFC 9562 version 7 value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid7(Uuid);

impl Uuid7 {
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl FromStr for Uuid7 {
    type Err = InvalidUuid;

    fn from_str(text: &str) -> Result<Self, InvalidUuid> {
        let uuid = Uuid::parse_str(text).map_err(|_| InvalidUuid)?;
        let canonical = uuid.hyphenated().to_string() == text;
        let is_v7 = uuid.get_version_num() == 7 && uuid.get_variant() == Variant::RFC4122;
        if canonical && is_v7 {
            Ok(Self(uuid))
        } else {
            Err(InvalidUuid)
        }
    }
}

impl fmt::Display for Uuid7 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

impl fmt::Debug for Uuid7 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uuid7({self})")
    }
}

impl serde::Serialize for Uuid7 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Uuid7 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}
