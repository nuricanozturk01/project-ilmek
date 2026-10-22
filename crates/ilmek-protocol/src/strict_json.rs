//! Strict JSON parsing (PROTOCOL.md §4; see docs/decisions/0002-json-profiles.md).
//!
//! Both profiles reject BOM, invalid UTF-8, duplicate keys and nesting beyond 16 levels.
//! [`Profile::Metadata`] additionally rejects floats, negative or >2^53-1 integers and
//! containers beyond 256 entries.

use std::cell::RefCell;
use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use thiserror::Error;

const MAX_DEPTH: usize = 16;
const MAX_ENTRIES: usize = 256;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JsonError {
    #[error("byte order mark is not allowed")]
    Bom,
    #[error("invalid UTF-8")]
    Utf8,
    #[error("duplicate object key `{0}`")]
    DuplicateKey(String),
    #[error("nesting deeper than {MAX_DEPTH}")]
    TooDeep,
    #[error("more than {MAX_ENTRIES} entries in one container")]
    TooManyEntries,
    #[error("number must be an integer in 0..=2^53-1")]
    Number,
    #[error("top-level value must be an object")]
    NotObject,
    #[error("malformed JSON: {0}")]
    Syntax(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Frame metadata: integers only, bounded containers.
    Metadata,
    /// Application bodies and `manifest.json`: any JSON number, size bounded by the frame.
    Body,
}

/// Parses frame metadata (the [`Profile::Metadata`] rules).
pub fn parse_object(bytes: &[u8]) -> Result<Map<String, Value>, JsonError> {
    parse_object_with(bytes, Profile::Metadata)
}

/// Parses `bytes` as a single strict JSON object under `profile`.
pub fn parse_object_with(bytes: &[u8], profile: Profile) -> Result<Map<String, Value>, JsonError> {
    if bytes.starts_with(BOM) {
        return Err(JsonError::Bom);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| JsonError::Utf8)?;
    let violation = RefCell::new(None);
    let seed = Strict {
        depth: 0,
        profile,
        violation: &violation,
    };

    let mut deserializer = serde_json::Deserializer::from_str(text);
    let parsed = seed.deserialize(&mut deserializer).and_then(|value| {
        deserializer.end()?;
        Ok(value)
    });

    if let Some(error) = violation.into_inner() {
        return Err(error);
    }
    match parsed.map_err(|e| JsonError::Syntax(e.to_string()))? {
        Value::Object(map) => Ok(map),
        _ => Err(JsonError::NotObject),
    }
}

/// Deserialization seed that records the first rule violation in `violation`, because serde
/// errors can only carry text.
#[derive(Clone, Copy)]
struct Strict<'a> {
    depth: usize,
    profile: Profile,
    violation: &'a RefCell<Option<JsonError>>,
}

impl Strict<'_> {
    fn reject<T, E: serde::de::Error>(self, error: JsonError) -> Result<T, E> {
        let message = error.to_string();
        self.violation.borrow_mut().get_or_insert(error);
        Err(E::custom(message))
    }

    fn child(self) -> Option<Self> {
        (self.depth < MAX_DEPTH).then_some(Self {
            depth: self.depth + 1,
            ..self
        })
    }
}

impl<'de> DeserializeSeed<'de> for Strict<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a strict JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
        if self.profile == Profile::Metadata && v > MAX_SAFE_INTEGER {
            return self.reject(JsonError::Number);
        }
        Ok(Value::Number(Number::from(v)))
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
        if self.profile == Profile::Body {
            return Ok(Value::Number(Number::from(v)));
        }
        match u64::try_from(v) {
            Ok(unsigned) => self.visit_u64(unsigned),
            Err(_) => self.reject(JsonError::Number),
        }
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        match (self.profile, Number::from_f64(v)) {
            (Profile::Body, Some(number)) => Ok(Value::Number(number)),
            _ => self.reject(JsonError::Number),
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let Some(child) = self.child() else {
            return self.reject(JsonError::TooDeep);
        };
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(child)? {
            if self.profile == Profile::Metadata && items.len() == MAX_ENTRIES {
                return self.reject(JsonError::TooManyEntries);
            }
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let Some(child) = self.child() else {
            return self.reject(JsonError::TooDeep);
        };
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if self.profile == Profile::Metadata && object.len() == MAX_ENTRIES {
                return self.reject(JsonError::TooManyEntries);
            }
            if object.contains_key(&key) {
                return self.reject(JsonError::DuplicateKey(key));
            }
            let value = map.next_value_seed(child)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}
