//! Requests an application page may make, and the checks every one of them passes.
//!
//! There is deliberately no field for an application id, a host, an address or a file path:
//! the page can only act on the application the runtime opened it for.

use ilmek_client::Error as ClientError;
use ilmek_core::session::ClientError as SessionError;
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{ErrorCode, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const MAX_INPUT_BYTES: usize = 200_000;
pub(crate) const MAX_STORAGE_VALUE_BYTES: usize = 256 * 1024;
pub(crate) const MIN_TIMEOUT_MS: u64 = 1_000;
pub(crate) const MAX_TIMEOUT_MS: u64 = 60_000;
const MAX_KEY_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(rename = "outcomeUnknown")]
    pub outcome_unknown: bool,
    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl Failure {
    pub(crate) fn invalid(message: &str) -> Self {
        Self::plain("INVALID_MESSAGE", message)
    }

    pub(crate) fn forbidden(message: &str) -> Self {
        Self::plain("FORBIDDEN", message)
    }

    pub(crate) fn plain(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable: false,
            outcome_unknown: false,
            operation_id: None,
        }
    }

    /// Maps a client error to what the page may see. `write` marks calls whose effect on the
    /// server is unknown when the answer is missing.
    pub(crate) fn from_client(
        error: &ClientError,
        write: bool,
        operation_id: Option<Uuid7>,
    ) -> Self {
        let operation_id = operation_id.map(|id| id.to_string());
        if let Some(remote) = error.remote() {
            let code = serde_json::to_value(remote.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "INTERNAL".to_owned());
            let unsure = write
                && matches!(
                    remote.code,
                    ErrorCode::Timeout | ErrorCode::AppUnavailable | ErrorCode::OperationPending
                );
            return Self {
                code,
                message: remote.message.to_string(),
                retryable: remote.retryable,
                outcome_unknown: unsure,
                operation_id: operation_id.filter(|_| unsure),
            };
        }
        let (code, message, unknown) = match error {
            ClientError::TrustStale => ("TRUST_STALE", "trust information is out of date", false),
            ClientError::PeerRevoked => (
                "PEER_REVOKED",
                "the server's certificate was revoked",
                false,
            ),
            ClientError::Session(SessionError::Timeout) => {
                ("TIMEOUT", "no answer; the outcome is unknown", true)
            }
            ClientError::Session(_) | ClientError::Tls(_) => {
                ("APP_UNAVAILABLE", "the server could not be reached", true)
            }
            _ => ("INTERNAL", "unexpected failure", false),
        };
        let unsure = unknown && write;
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable: unknown,
            outcome_unknown: unsure,
            operation_id: operation_id.filter(|_| unsure),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Response {
    Success { ok: bool, result: Value },
    Failure { ok: bool, error: Failure },
}

impl Response {
    pub(crate) fn success(result: Value) -> Self {
        Self::Success { ok: true, result }
    }

    pub(crate) fn failure(error: Failure) -> Self {
        Self::Failure { ok: false, error }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallRequest {
    v: u8,
    pub(crate) method: String,
    pub(crate) input: Value,
    pub(crate) write: bool,
    #[serde(rename = "operationId")]
    pub(crate) operation_id: Option<String>,
    #[serde(rename = "timeoutMs")]
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscribeRequest {
    v: u8,
    pub(crate) topic: String,
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsubscribeRequest {
    v: u8,
    #[serde(rename = "subscriptionId")]
    pub(crate) subscription_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyRequest {
    v: u8,
    pub(crate) key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRequest {
    v: u8,
    pub(crate) key: String,
    pub(crate) value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BareRequest {
    v: u8,
}

/// A request that passed structural parsing; permissions are checked by the session.
#[derive(Debug)]
pub enum Request {
    Call(CallRequest),
    Subscribe(SubscribeRequest),
    Unsubscribe(UnsubscribeRequest),
    StorageGet(KeyRequest),
    StorageSet(SetRequest),
    StorageRemove(KeyRequest),
    ConnectionState,
}

fn typed<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, Failure> {
    serde_json::from_value(value)
        .map_err(|_| Failure::invalid("request does not match its operation"))
}

impl Request {
    /// Parses and structurally validates a page request. Unknown operations, unknown fields and
    /// protocol versions other than 1 are rejected.
    pub fn parse(raw: Value) -> Result<Self, Failure> {
        let operation = raw
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| Failure::invalid("missing operation"))?
            .to_owned();
        let mut fields = raw;
        if let Some(object) = fields.as_object_mut() {
            object.remove("op");
        }
        let (request, version) = match operation.as_str() {
            "call" => {
                let r: CallRequest = typed(fields)?;
                let v = r.v;
                (Self::Call(r), v)
            }
            "subscribe" => {
                let r: SubscribeRequest = typed(fields)?;
                let v = r.v;
                (Self::Subscribe(r), v)
            }
            "unsubscribe" => {
                let r: UnsubscribeRequest = typed(fields)?;
                let v = r.v;
                (Self::Unsubscribe(r), v)
            }
            "storage.get" => {
                let r: KeyRequest = typed(fields)?;
                let v = r.v;
                (Self::StorageGet(r), v)
            }
            "storage.set" => {
                let r: SetRequest = typed(fields)?;
                let v = r.v;
                (Self::StorageSet(r), v)
            }
            "storage.remove" => {
                let r: KeyRequest = typed(fields)?;
                let v = r.v;
                (Self::StorageRemove(r), v)
            }
            "connection.state" => (Self::ConnectionState, typed::<BareRequest>(fields)?.v),
            _ => return Err(Failure::invalid("unknown operation")),
        };
        if version == 1 {
            Ok(request)
        } else {
            Err(Failure::invalid("unsupported bridge version"))
        }
    }
}

impl CallRequest {
    pub(crate) fn validated_method(&self) -> Result<Method, Failure> {
        self.method
            .parse()
            .map_err(|_| Failure::invalid("method name is not valid"))
    }

    pub(crate) fn validated_operation_id(&self) -> Result<Option<Uuid7>, Failure> {
        match (&self.operation_id, self.write) {
            (Some(text), true) => text
                .parse()
                .map(Some)
                .map_err(|_| Failure::invalid("operationId must be a UUIDv7")),
            (None, true) => Err(Failure::invalid("writes need an operationId")),
            (Some(_), false) => Err(Failure::invalid("reads must not carry an operationId")),
            (None, false) => Ok(None),
        }
    }

    pub(crate) fn body(&self) -> Result<Vec<u8>, Failure> {
        if !self.input.is_object() {
            return Err(Failure::invalid("input must be a JSON object"));
        }
        let bytes = serde_json::to_vec(&self.input).map_err(|_| Failure::invalid("input"))?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(Failure::plain("TOO_LARGE", "input is too large"));
        }
        Ok(bytes)
    }

    pub(crate) fn timeout(&self) -> Result<Option<std::time::Duration>, Failure> {
        match self.timeout_ms {
            None => Ok(None),
            Some(ms) if (MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&ms) => {
                Ok(Some(std::time::Duration::from_millis(ms)))
            }
            Some(_) => Err(Failure::invalid("timeoutMs must be between 1000 and 60000")),
        }
    }
}

/// Storage keys: 1..=128 characters of `[A-Za-z0-9._:-]`.
pub(crate) fn valid_key(key: &str) -> Result<(), Failure> {
    let ok = !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(Failure::invalid("storage key is not valid"))
    }
}
