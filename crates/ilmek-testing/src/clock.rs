use std::sync::atomic::{AtomicU64, Ordering};

use ilmek_core::clock::{Clock, SystemClock};

/// A clock tests can move forward. It starts at the real time so certificates stay valid.
#[derive(Debug)]
pub struct ManualClock(AtomicU64);

impl ManualClock {
    #[must_use]
    pub fn starting_now() -> Self {
        Self(AtomicU64::new(SystemClock.now_ms()))
    }

    pub fn advance_ms(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::Relaxed);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
