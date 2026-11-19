use std::{
    future::Future,
    sync::OnceLock,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};

/// Base offset for monotonic instants so `checked_sub` behaves like an OS
/// uptime clock and does not underflow for recently-modified files at startup.
pub const MONOTONIC_EPOCH_OFFSET: Duration = Duration::from_hours(365 * 24 * 100);

/// A monotonic timestamp independent of OS clock types, suitable for both
/// production (`SystemClock`) and deterministic simulation (`SimClock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonotonicInstant(Duration);

impl MonotonicInstant {
    /// Construct a `MonotonicInstant` from a duration since the clock's origin.
    #[must_use]
    pub const fn from_duration(duration: Duration) -> Self {
        Self(duration)
    }

    /// Return the underlying duration since the clock's origin.
    #[must_use]
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    /// Add a duration, returning `None` on overflow.
    #[must_use]
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.0.checked_add(duration).map(Self)
    }

    /// Subtract a duration, returning `None` on underflow.
    #[must_use]
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        self.0.checked_sub(duration).map(Self)
    }

    /// Returns the amount of time elapsed from `earlier` to `self`, saturating at zero.
    #[must_use]
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

/// Abstraction over monotonic time, wall-clock time, and async sleeping.
pub trait Clock: Clone + Send + Sync + 'static {
    /// Returns the current monotonic instant.
    fn now(&self) -> MonotonicInstant;

    /// Returns the current UTC wall-clock timestamp.
    fn now_utc(&self) -> DateTime<Utc>;

    /// Sleeps for the given duration.
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
}

/// Production [`Clock`] backed by the OS monotonic clock, `Utc::now()`, and `tokio::time::sleep`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

static SYSTEM_EPOCH: OnceLock<Instant> = OnceLock::new();

impl Clock for SystemClock {
    fn now(&self) -> MonotonicInstant {
        let epoch = SYSTEM_EPOCH.get_or_init(Instant::now);
        MonotonicInstant(MONOTONIC_EPOCH_OFFSET.saturating_add(epoch.elapsed()))
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}
