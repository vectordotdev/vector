use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use metrics::Counter;
use pin_project::pin_project;

/// An opaque snapshot of thread CPU time.
///
/// On Linux and macOS this uses `CLOCK_THREAD_CPUTIME_ID`, which measures
/// only the time the calling thread was actually scheduled on a CPU (true CPU
/// time, excluding preemption and context switches to other threads/processes).
///
/// On Windows this uses `GetThreadTimes`, which provides the same guarantee
/// with 100ns granularity.
///
/// On other platforms thread CPU time is unavailable; [`ThreadTime`] is a
/// no-op that always reports zero elapsed time. The per-component CPU metric
/// is omitted on those platforms (see [`register_counter`]) rather than
/// emitted with misleading wall-clock or zero values.
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

// ── Windows: GetThreadTimes ───────────────────────────────────────────────

#[cfg(target_os = "windows")]
struct Inner(Duration);

#[cfg(target_os = "windows")]
impl Inner {
    fn now() -> Self {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut user = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };

        // SAFETY:
        // - `GetCurrentThread()` returns a pseudo-handle that is always valid
        //   and does not need to be closed.
        // - All four `FILETIME` pointers are valid, properly aligned, and
        //   stack-allocated.
        // - The return value is intentionally ignored: failure is only possible
        //   with an invalid handle, which cannot occur with `GetCurrentThread()`.
        unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            );
        }

        // Combine the low/high halves of each FILETIME into a u64, then sum
        // kernel + user. FILETIME units are 100-nanosecond intervals.
        let kernel_ns = filetime_to_nanos(kernel);
        let user_ns = filetime_to_nanos(user);
        Inner(Duration::from_nanos(kernel_ns + user_ns))
    }

    #[inline]
    fn elapsed(&self) -> Duration {
        Self::now().0.saturating_sub(self.0)
    }
}

#[cfg(target_os = "windows")]
#[inline]
fn filetime_to_nanos(ft: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    ticks * 100 // convert 100ns intervals to nanoseconds
}

// ── Other platforms: no-op (metric is not emitted on these platforms) ─────

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
struct Inner;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl Inner {
    #[inline]
    fn now() -> Self {
        Inner
    }

    #[inline]
    fn elapsed(&self) -> Duration {
        Duration::ZERO
    }
}

// ── CpuTimedFuture: per-poll CPU time accumulator ─────────────────────────

/// A [`Future`] adapter that accumulates thread CPU time across every `poll`.
///
/// Each call to [`Future::poll`] is bracketed by a [`ThreadTime`] sample;
/// the delta is added to `counter`. Tokio's executor cannot migrate a task
/// to another worker thread or run another task on the current thread between
/// the entry and exit of a single `poll`, so each delta is a clean per-thread
/// CPU-time measurement of the wrapped future's work for that poll. Multiple
/// polls (across `Pending` returns and wake-ups) accumulate into the same
/// counter, with each poll independently sampling the thread it ran on.
///
/// This is the per-task analogue of tokio's unstable
/// `on_before_task_poll` / `on_after_task_poll` runtime hooks: it hooks the
/// same boundary, but on a single future rather than the whole runtime, and
/// it works on stable Rust without `--cfg tokio_unstable`.
///
/// # Measurement scope and upstream isolation
///
/// Vector components communicate only through `BufferReceiver`/`BufferSender`
/// channels — never through stream combinators chained across component
/// boundaries. Each component runs in its own tokio task. When a transform
/// polls its input channel, it dequeues items that the upstream component
/// computed earlier, in its own task; it does **not** execute any upstream
/// code. The upstream's CPU was already charged to the upstream's counter at
/// the time those items were produced. This holds even when the channel is
/// always full: the items were produced by upstream CPU that was already
/// counted upstream; we are only dequeuing them.
///
/// As a consequence, this counter for a transform task **includes**:
///
/// - Input-channel dequeue (our task's poll of the channel, not upstream work)
/// - `on_events_received` bookkeeping and metric emit
/// - `transform_all` (the core CPU cost)
/// - `send_outputs` / fanout dispatch to downstream channels
/// - Per-event schema validation and latency recording
/// - For transforms that spawn helper tasks (e.g. `aws_ec2_metadata` IMDS
///   refresh, `throttle`'s flush loop): those tasks' polls, via
///   [`spawn_timed`], feed the **same** counter rather than being silently
///   excluded.
///
/// And **does not** include:
///
/// - Other components' CPU — channel isolation guarantees this.
/// - Time the task is parked (`Poll::Pending`): no polls → no measurement.
///   Back-pressure and input starvation show up as flat counter growth.
/// - `Drop` of the inner future after the final `Poll::Ready`. The drop runs
///   after `CpuTimedFuture::poll` returns, so it is outside the timed window.
///   This is a one-time cost at task shutdown, not steady-state.
/// - Tokio scheduler and waker overhead — executor work, not component work.
///
/// # Concurrent sync transforms
///
/// For transforms that run concurrently (`enable_concurrency() == true`), both
/// the driver future **and** each per-batch spawned task are wrapped with this
/// adapter. Because the spawned tasks are separate tokio tasks, the driver's
/// `CpuTimedFuture` never observes their polls — there is no double-counting.
/// The driver is measured for: input dequeue, `on_events_received`, and
/// `send_outputs`. Each spawned task is measured for: `transform_all`.
///
/// Construct it via [`CpuTimedExt::cpu_timed`].
#[pin_project]
pub(crate) struct CpuTimedFuture<F> {
    #[pin]
    inner: F,
    counter: Counter,
}

impl<F: Future> Future for CpuTimedFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let t0 = ThreadTime::now();
        let this = self.project();
        let result = this.inner.poll(cx);
        this.counter.increment(t0.elapsed().as_nanos() as u64);
        result
    }
}

/// Extension trait that wraps a future in [`CpuTimedFuture`] via a chained
/// call:
///
/// ```ignore
/// async move { /* work */ }.cpu_timed(counter)
/// ```
///
/// Mirrors the style of [`tracing::Instrument::in_current_span`].
pub(crate) trait CpuTimedExt: Future + Sized {
    fn cpu_timed(self, counter: Counter) -> CpuTimedFuture<Self> {
        CpuTimedFuture {
            inner: self,
            counter,
        }
    }
}

impl<F: Future> CpuTimedExt for F {}

/// Spawns `future` on the current tokio runtime with CPU-time accounting
/// attached, equivalent to:
///
/// ```ignore
/// tokio::spawn(future.cpu_timed(counter))
/// ```
///
/// Use this when spawning background tasks (e.g. a transform's housekeeping
/// loop) whose CPU usage should be attributed back to a component. Wrap the
/// future with [`tracing::Instrument`] (or similar adapters) before passing
/// it in if you want those adapters' per-poll work included in the CPU-time
/// measurement.
pub(crate) fn spawn_timed<F>(future: F, counter: Counter) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future.cpu_timed(counter))
}

/// Registers the `component_cpu_usage_ns_total` counter for the calling
/// component on platforms where thread CPU time is available (Linux, macOS,
/// Windows). On other platforms it returns [`Counter::noop()`] — the metric
/// is **not** emitted at all, rather than reporting wall-clock or zero values
/// that would be misleading to compare against supported platforms.
///
/// Call this inside a tracing span that carries `component_id`,
/// `component_kind`, and `component_type` labels so that those labels are
/// automatically attached to the registered counter by the metrics recorder.
///
/// # Using the emitted counter
///
/// The counter is cumulative nanoseconds of CPU time. To derive the average
/// number of CPU cores a component consumed over a window:
///
/// ```promql
/// rate(component_cpu_usage_ns_total{component_id="my_remap"}[1m]) / 1e9
/// ```
///
/// This value can exceed 1.0 when a transform genuinely uses multiple cores
/// (concurrent execution path). Compare against `utilization` to separate
/// CPU cost from pipeline back-pressure.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn register_counter() -> Counter {
    vector_lib::counter!(vector_lib::internal_event::CounterName::ComponentCpuUsageNsTotal)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn register_counter() -> Counter {
    Counter::noop()
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
        assert!(
            second >= first,
            "clock went backwards: {second:?} < {first:?}"
        );
    }
}
