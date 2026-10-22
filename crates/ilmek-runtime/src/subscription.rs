//! Event subscriptions that survive connection loss.
//!
//! Each event is handed to the page, its cursor is stored, and only then is it acknowledged to
//! the server. After a disconnect the subscription resumes from the stored cursor, and event
//! ids already delivered are skipped, so the page sees every event at least once and, in
//! practice, once.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use ilmek_client::{Delivered, Error, EventSubscription};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Cursor, ErrorCode, Method};
use tokio_util::sync::CancellationToken;

use crate::bridge::Failure;
use crate::events::BridgeEvent;
use crate::session::AppSession;

const REMEMBERED_EVENTS: usize = 256;
const BACKOFF: [Duration; 5] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];

#[derive(Debug)]
pub(crate) struct Subscription {
    pub(crate) stop: CancellationToken,
}

/// Opens the first connection synchronously, so permission and topic errors reach the page as
/// the answer to its `subscribe` call, then keeps the stream alive in the background.
pub(crate) async fn start(
    session: Arc<AppSession>,
    topic: Method,
    cursor: Option<Cursor>,
) -> Result<Uuid7, Failure> {
    let first = session
        .client()
        .subscribe(topic.as_str(), cursor)
        .await
        .map_err(|error| Failure::from_client(&error, false, None))?;
    session.note_reachability(true);

    let id = Uuid7::generate();
    let stop = session.shutdown.child_token();
    session.register_subscription(id, Subscription { stop: stop.clone() });
    tokio::spawn(keep_alive(session, id, topic, first, stop));
    Ok(id)
}

struct Delivery {
    seen: HashSet<Uuid7>,
    order: VecDeque<Uuid7>,
}

impl Delivery {
    fn first_time(&mut self, event_id: Uuid7) -> bool {
        if !self.seen.insert(event_id) {
            return false;
        }
        self.order.push_back(event_id);
        if self.order.len() > REMEMBERED_EVENTS {
            let oldest = self.order.pop_front();
            if let Some(old) = oldest {
                self.seen.remove(&old);
            }
        }
        true
    }
}

enum Ended {
    Stopped,
    Lost,
}

async fn keep_alive(
    session: Arc<AppSession>,
    id: Uuid7,
    topic: Method,
    first: EventSubscription,
    stop: CancellationToken,
) {
    let mut delivery = Delivery {
        seen: HashSet::new(),
        order: VecDeque::new(),
    };
    let mut current = Some(first);
    let mut attempt = 0usize;
    let final_error = loop {
        let subscription = match current.take() {
            Some(subscription) => subscription,
            None => match resubscribe(&session, &topic, &stop, &mut attempt).await {
                Ok(Some(subscription)) => subscription,
                Ok(None) => break None,
                Err(failure) => break Some(failure),
            },
        };
        match pump(&session, id, &topic, subscription, &mut delivery, &stop).await {
            Ended::Stopped => break None,
            Ended::Lost => {
                session.note_reachability(false);
                attempt = 0;
            }
        }
    };
    session.forget_subscription(id);
    if final_error.is_some() || !stop.is_cancelled() {
        session.sink.emit(&BridgeEvent::SubscriptionEnded {
            subscription_id: id,
            error: final_error,
        });
    }
}

/// Reconnects with backoff. `Ok(None)` means the subscription was cancelled meanwhile.
async fn resubscribe(
    session: &Arc<AppSession>,
    topic: &Method,
    stop: &CancellationToken,
    attempt: &mut usize,
) -> Result<Option<EventSubscription>, Failure> {
    loop {
        let delay = BACKOFF[(*attempt).min(BACKOFF.len() - 1)];
        *attempt += 1;
        tokio::select! {
            () = stop.cancelled() => return Ok(None),
            () = tokio::time::sleep(delay) => {}
        }
        let resume = session
            .storage
            .cursor(topic.to_string())
            .await
            .ok()
            .flatten()
            .and_then(|text| text.parse::<Cursor>().ok());
        match session.client().subscribe(topic.as_str(), resume).await {
            Ok(subscription) => {
                session.note_reachability(true);
                return Ok(Some(subscription));
            }
            Err(error) if error.remote_code() == Some(ErrorCode::CursorExpired) => {
                return Err(Failure::from_client(&error, false, None));
            }
            Err(error) if error.remote().is_some() => {
                return Err(Failure::from_client(&error, false, None));
            }
            Err(_) => {}
        }
    }
}

async fn pump(
    session: &Arc<AppSession>,
    id: Uuid7,
    topic: &Method,
    mut subscription: EventSubscription,
    delivery: &mut Delivery,
    stop: &CancellationToken,
) -> Ended {
    loop {
        let next = tokio::select! {
            () = stop.cancelled() => {
                let _ = subscription.unsubscribe().await;
                return Ended::Stopped;
            }
            next = subscription.next() => next,
        };
        let Ok(Some(delivered)) = next else {
            return Ended::Lost;
        };
        if hand_over(session, id, topic, &subscription, &delivered, delivery)
            .await
            .is_err()
        {
            return Ended::Lost;
        }
    }
}

async fn hand_over(
    session: &Arc<AppSession>,
    id: Uuid7,
    topic: &Method,
    subscription: &EventSubscription,
    delivered: &Delivered,
    delivery: &mut Delivery,
) -> Result<(), Error> {
    if delivery.first_time(delivered.event.event_id) {
        let body = serde_json::from_slice(&delivered.body).unwrap_or(serde_json::Value::Null);
        session.sink.emit(&BridgeEvent::Event {
            subscription_id: id,
            event: body,
        });
    }
    // Store before acknowledging: if we crash in between, the server redelivers.
    let _ = session
        .storage
        .save_cursor(topic.to_string(), delivered.event.cursor.to_string())
        .await;
    subscription.acknowledge(delivered).await
}
