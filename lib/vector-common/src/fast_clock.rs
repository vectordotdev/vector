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
        atomic::{AtomicI64, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Cadence at which the background updater refreshes the cached timestamp.
const TICK: Duration = Duration::from_millis(25);

static RECENT_MS: AtomicU64 = AtomicU64::new(0);
static RECENT_UNIX_MS: AtomicI64 = AtomicI64::new(0);
static INIT: OnceLock<()> = OnceLock::new();

fn unix_millis_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(i64::MAX)
}

fn ensure_init() {
    INIT.get_or_init(|| {
        let epoch = Instant::now();
        // Pre-populate the cached values so the very first reader does not
        // observe 0 before the updater thread has had a chance to tick.
        RECENT_UNIX_MS.store(unix_millis_now(), Ordering::Relaxed);
        std::thread::Builder::new()
            .name("fast-clock".into())
            .spawn(move || {
                loop {
                    let elapsed_ms = u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
                    RECENT_MS.store(elapsed_ms, Ordering::Relaxed);
                    RECENT_UNIX_MS.store(unix_millis_now(), Ordering::Relaxed);
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

/// Returns the cached count of milliseconds since the Unix epoch.
///
/// Resolution is at most `TICK` (currently 25ms) — the value is refreshed
/// from `SystemTime::now()` on each tick, so `recent_unix_millis()` will
/// lag real wall-clock time by up to that cadence. Suitable for
/// histogram-style metrics like source lag time, where ms precision and
/// up-to-25ms staleness are both acceptable.
///
/// Cost is a single relaxed atomic load.
#[must_use]
pub fn recent_unix_millis() -> i64 {
    ensure_init();
    RECENT_UNIX_MS.load(Ordering::Relaxed)
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

    #[test]
    fn unix_millis_is_close_to_systemtime() {
        init();
        // Allow a tick to populate the cached value.
        std::thread::sleep(Duration::from_millis(50));
        let cached = recent_unix_millis();
        let truth = unix_millis_now();
        let drift = (truth - cached).abs();
        // Worst case is roughly TICK + scheduling jitter; allow 200ms.
        assert!(
            drift < 200,
            "drift too large: cached={cached} truth={truth}"
        );
    }
}
