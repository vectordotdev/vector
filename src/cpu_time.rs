use std::time::Duration;

/// An opaque snapshot of thread CPU time.
///
/// On Linux and macOS this uses `CLOCK_THREAD_CPUTIME_ID`, which measures
/// only the time the calling thread was actually scheduled on a CPU (true CPU
/// time, excluding preemption and context switches to other threads/processes).
///
/// On other platforms this falls back to wall-clock time via
/// [`std::time::Instant`].
///
/// # Usage
///
/// Call [`ThreadTime::now`] immediately before the work to measure, then call
/// [`ThreadTime::elapsed`] immediately after:
///
/// ```ignore
/// let t0 = ThreadTime::now();
/// do_work();
/// let cpu_time = t0.elapsed();
/// ```
///
/// # Correctness for sync transforms
///
/// This measurement is accurate for [`crate::transforms::SyncTransform`]
/// because `transform_all` is synchronous and non-yielding: between the two
/// measurement points the worker thread runs only transform code, with no
/// `.await` points that could interleave other tokio tasks.
pub(crate) struct ThreadTime(Inner);

impl ThreadTime {
    /// Captures the current thread CPU time.
    #[inline]
    pub(crate) fn now() -> Self {
        ThreadTime(Inner::now())
    }

    /// Returns the CPU time elapsed since this snapshot was taken.
    #[inline]
    pub(crate) fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

// ── Linux / macOS: CLOCK_THREAD_CPUTIME_ID ────────────────────────────────

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Inner(Duration);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Inner {
    fn now() -> Self {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY:
        // - `ts` is a valid, zero-initialised `timespec` on the stack.
        // - `CLOCK_THREAD_CPUTIME_ID` is a valid clock ID on Linux ≥ 2.6 and
        //   macOS ≥ 10.12 (both are baseline requirements for Vector).
        // - The return value is intentionally ignored: the only failure modes
        //   are an invalid clock ID (not the case here) or an invalid pointer
        //   (not the case here), neither of which can occur.
        unsafe {
            libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts);
        }
        Inner(Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32))
    }

    #[inline]
    fn elapsed(&self) -> Duration {
        Self::now().0.saturating_sub(self.0)
    }
}

// ── Other platforms: wall-clock fallback ──────────────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
struct Inner(std::time::Instant);

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl Inner {
    fn now() -> Self {
        Inner(std::time::Instant::now())
    }

    #[inline]
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_is_non_negative() {
        let t0 = ThreadTime::now();
        // Burn a small amount of CPU to ensure the clock advances.
        let _: u64 = (0u64..10_000).sum();
        assert!(t0.elapsed() >= Duration::ZERO);
    }

    #[test]
    fn elapsed_is_monotone() {
        // Two consecutive elapsed() calls on the same snapshot must be
        // non-decreasing (the clock never goes backwards).
        let t0 = ThreadTime::now();
        let _: u64 = (0u64..10_000).sum();
        let first = t0.elapsed();
        let _: u64 = (0u64..10_000).sum();
        let second = t0.elapsed();
        assert!(second >= first, "clock went backwards: {second:?} < {first:?}");
    }
}
