use std::future::Future;
use std::time::Duration;

use crate::error::Error;

/// Backoff for writes whose outcome is unknown (PROTOCOL.md §13): at most five more attempts,
/// each with the *same* operation id and body.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub delays: Vec<Duration>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            delays: [500, 1000, 2000, 4000, 8000]
                .map(Duration::from_millis)
                .to_vec(),
        }
    }
}

fn jittered(delay: Duration) -> Duration {
    let mut byte = [0u8; 1];
    // Jitter is best effort; without randomness the plain delay is used.
    if getrandom::fill(&mut byte).is_err() {
        return delay;
    }
    // Between 50% and 100% of the nominal delay.
    delay.mul_f64(0.5 + f64::from(byte[0]) / 510.0)
}

/// Runs `attempt` until it succeeds, fails definitively, or the policy is exhausted.
///
/// Only failures where the outcome is unknown, or the server asked for a retry, are repeated.
/// The caller must build the request (and its operation id) once, outside the closure.
pub async fn retry_write<T, F, Fut>(policy: &RetryPolicy, mut attempt: F) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut delays = policy.delays.iter();
    loop {
        let error = match attempt().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        let wait = if let Some(remote) = error.remote() {
            if !remote.retryable {
                return Err(error);
            }
            remote.retry_after_ms.map(Duration::from_millis)
        } else if error.outcome_unknown() {
            None
        } else {
            return Err(error);
        };
        let Some(delay) = delays.next() else {
            return Err(error);
        };
        tokio::time::sleep(wait.unwrap_or_else(|| jittered(*delay))).await;
    }
}
