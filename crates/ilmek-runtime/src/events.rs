use ilmek_protocol::Uuid7;
use serde::Serialize;
use serde_json::Value;

use crate::bridge::Failure;

/// Pushed from the runtime to the application page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BridgeEvent {
    Connection {
        state: &'static str,
    },
    Event {
        #[serde(rename = "subscriptionId")]
        subscription_id: Uuid7,
        event: Value,
    },
    SubscriptionEnded {
        #[serde(rename = "subscriptionId")]
        subscription_id: Uuid7,
        error: Option<Failure>,
    },
}

/// Receives events destined for one application page.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &BridgeEvent);
}
