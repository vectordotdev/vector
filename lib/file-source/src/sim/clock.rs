use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use file_source_common::{Clock, MonotonicInstant, clock::MONOTONIC_EPOCH_OFFSET};

#[derive(Debug)]
struct SimClockInner {
    monotonic: MonotonicInstant,
    wall: DateTime<Utc>,
}

/// Deterministic in-memory [`Clock`] for simulation testing.
#[derive(Debug, Clone)]
pub struct SimClock {
    inner: Arc<Mutex<SimClockInner>>,
}

impl Default for SimClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SimClock {
    /// Creates a new `SimClock` initialized at a fixed deterministic epoch.
    #[must_use]
    pub fn new() -> Self {
        let wall = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid epoch timestamp");
        Self {
            inner: Arc::new(Mutex::new(SimClockInner {
                monotonic: MonotonicInstant::from_duration(MONOTONIC_EPOCH_OFFSET),
                wall,
            })),
        }
    }

    /// Advances both monotonic and wall clocks by `duration`.
    pub fn advance(&self, duration: Duration) {
        let mut inner = self.inner.lock().unwrap();
        inner.monotonic = inner
            .monotonic
            .checked_add(duration)
            .expect("sim monotonic clock overflow");
        inner.wall += chrono::Duration::from_std(duration).expect("duration fits chrono");
    }

    /// Adjusts only the wall clock (useful for testing clock skew / `ignore_older`).
    #[allow(dead_code)]
    pub fn set_wall(&self, wall: DateTime<Utc>) {
        self.inner.lock().unwrap().wall = wall;
    }
}

impl Clock for SimClock {
    fn now(&self) -> MonotonicInstant {
        self.inner.lock().unwrap().monotonic
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.inner.lock().unwrap().wall
    }

    async fn sleep(&self, duration: Duration) {
        self.advance(duration);
        tokio::task::yield_now().await;
    }
}
