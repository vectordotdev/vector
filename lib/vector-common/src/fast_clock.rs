//! A coarse, cached monotonic clock for hot-path metric instrumentation.
//!
//! Reading [`recent_millis`] is a single relaxed atomic load, which is
//! significantly cheaper than `std::time::Instant::now()` on platforms where
//! the latter goes through a vDSO-backed `clock_gettime`. The tradeoff is
//! resolution: the value is updated by a background thread on a fixed cadence
//! (see [`TICK`]), so it lags real time by up to that cadence.
//!
//! Intended for histogram binning and lag-time computations where
//! millisecond resolution is sufficient. Do not use where ordering between
//! events on the same path matters at sub-tick granularity.

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Cadence at which the background updater refreshes the cached timestamp.
const TICK: Duration = Duration::from_millis(25);

static RECENT_MS: AtomicU64 = AtomicU64::new(0);
static INIT: OnceLock<()> = OnceLock::new();

fn ensure_init() {
    INIT.get_or_init(|| {
        let epoch = Instant::now();
        std::thread::Builder::new()
            .name("fast-clock".into())
            .spawn(move || {
                loop {
                    let elapsed_ms = u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
                    RECENT_MS.store(elapsed_ms, Ordering::Relaxed);
                    std::thread::sleep(TICK);
                }
            })
            .expect("failed to spawn fast-clock updater thread");
    });
}

/// Eagerly start the background updater. Optional: [`recent_millis`] will
/// auto-initialize on first call. Calling this from `main` removes the
/// auto-init branch from the very first reader.
pub fn init() {
    ensure_init();
}

/// Returns the cached count of milliseconds since the clock was first
/// initialized. Cost is a single relaxed atomic load (plus a `OnceLock`
/// fast-path check on first call).
///
/// Resolution is at most `TICK` (currently 25ms). Returns `0` if called
/// before the first updater tick has executed; callers that compute
/// elapsed durations should be tolerant of this initial-zero case.
#[must_use]
pub fn recent_millis() -> u64 {
    ensure_init();
    RECENT_MS.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_forward() {
        init();
        std::thread::sleep(Duration::from_millis(100));
        let a = recent_millis();
        std::thread::sleep(Duration::from_millis(100));
        let b = recent_millis();
        assert!(b > a, "expected b ({b}) > a ({a})");
        assert!(b - a >= 50, "expected at least 50ms elapsed, got {}", b - a);
    }
}
