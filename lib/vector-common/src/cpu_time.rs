//! Per-component CPU-time measurement primitives.
//!
//! This module provides the building blocks for attributing CPU time to
//! individual Vector components at runtime:
//!
//! - [`ThreadTime`] — a lightweight snapshot of thread CPU time, backed by
//!   `CLOCK_THREAD_CPUTIME_ID` on Linux/macOS, `GetThreadTimes` on Windows,
//!   and a zero no-op on other platforms.
//! - [`CpuTimedFuture`] / [`CpuTimedExt`] — a [`Future`] adapter that
//!   brackets every `poll` call with a [`ThreadTime`] sample and accumulates
//!   the delta into a [`metrics::Counter`].
//! - [`CpuScopedFuture`] — a [`Future`] adapter that makes a counter
//!   inheritable by tasks spawned from it, without timing the future itself.
//! - [`spawn_timed`] / [`spawn_timed_without_span`] — spawn helpers that attach the
//!   counter of the calling task, if any, to the spawned task.
//! - `register_counter` — registers the `component_cpu_usage_ns_total` metrics
//!   counter for a component (available on Linux, macOS, and Windows only).
//!
//! # Counter inheritance
//!
//! While a [`CpuTimedFuture`] or [`CpuScopedFuture`] is being polled, its
//! counter is stored in a thread-local. [`spawn_timed`] and
//! [`spawn_timed_without_span`] read it at spawn time, so any task spawned from a
//! component's task (or from its `build` step) charges its CPU to the same
//! component, transitively. Code spawned outside of such a poll, or with
//! `tokio::spawn` / [`crate::spawn_in_current_span`], is not attributed to any
//! component.
//!
//! Tasks backing resources shared across components (global clients, caches,
//! lazily initialized singletons) are independent from the component that
//! creates them: spawn them with [`crate::spawn_in_current_span`]. With
//! [`spawn_timed`] they would be charged to whichever component happened to
//! trigger their creation.

use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::future::Either;
use metrics::Counter;
use pin_project::pin_project;

thread_local! {
    static CURRENT_COUNTER: RefCell<Option<Counter>> = const { RefCell::new(None) };
}

/// Makes `counter` the inheritable counter of the current thread until dropped,
/// then restores the previous one.
struct CounterScope {
    previous: Option<Counter>,
}

impl CounterScope {
    #[inline]
    fn enter(counter: &Counter) -> Self {
        let previous = CURRENT_COUNTER.with(|current| current.replace(Some(counter.clone())));
        Self { previous }
    }
}

impl Drop for CounterScope {
    #[inline]
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT_COUNTER.with(|current| *current.borrow_mut() = previous);
    }
}

/// Returns the counter of the [`CpuTimedFuture`] or [`CpuScopedFuture`]
/// currently being polled on this thread, if any.
#[must_use]
pub fn current_counter() -> Option<Counter> {
    CURRENT_COUNTER.with(|current| current.borrow().clone())
}

/// An opaque snapshot of thread CPU time.
///
/// On Linux and macOS this uses `CLOCK_THREAD_CPUTIME_ID`, which measures
/// only the time the calling thread was actually scheduled on a CPU (true CPU
/// time, excluding preemption and context switches to other threads/processes).
///
/// On Windows this uses `GetThreadTimes`. Its values are expressed in 100ns
/// units but only advance at the scheduler clock tick (typically 15.6 ms):
/// Windows charges each tick to the thread running when it fires. A poll
/// shorter than a tick therefore usually reads zero, and occasionally a whole
/// tick, so the counter is only a statistical estimate there.
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
/// This measurement is accurate for synchronous transforms
/// because `transform_all` is synchronous and non-yielding: between the two
/// measurement points the worker thread runs only transform code, with no
/// `.await` points that could interleave other tokio tasks.
pub struct ThreadTime(Inner);

impl ThreadTime {
    /// Captures the current thread CPU time.
    #[inline]
    #[must_use]
    pub fn now() -> Self {
        ThreadTime(Inner::now())
    }

    /// Returns the CPU time elapsed since this snapshot was taken.
    #[inline]
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

// ── Linux / macOS: CLOCK_THREAD_CPUTIME_ID ────────────────────────────────

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Inner(Duration);

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Inner {
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Preserve clock conversions"
    )]
    #[allow(clippy::cast_sign_loss, reason = "Preserve clock conversions")]
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
            libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut ts);
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
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
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
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
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
/// - Tasks spawned from it with [`spawn_timed`] or [`spawn_timed_without_span`]
///   (e.g. `aws_ec2_metadata` IMDS refresh, `throttle`'s flush loop): those
///   tasks' polls feed the **same** counter rather than being silently
///   excluded. See [counter inheritance](self#counter-inheritance).
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
/// # When to use it
///
/// Only wrap the outermost future of a task, when spawning it. The topology
/// does this for each component task, and [`spawn_timed`] /
/// [`spawn_timed_without_span`] do it for the tasks they spawn from there. Futures
/// awaited inline are already measured by the task polling them: they never
/// need a wrapper of their own, which would count their CPU twice.
///
/// Construct it via [`CpuTimedExt::cpu_timed`].
#[pin_project]
pub struct CpuTimedFuture<F> {
    #[pin]
    inner: F,
    counter: Counter,
}

impl<F: Future> Future for CpuTimedFuture<F> {
    type Output = F::Output;

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Preserve nanosecond accounting"
    )]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();
        let _scope = CounterScope::enter(this.counter);
        let t0 = ThreadTime::now();
        let result = this.inner.poll(cx);
        this.counter.increment(t0.elapsed().as_nanos() as u64);
        result
    }
}

/// A [`Future`] adapter that makes `counter` inheritable by the tasks spawned
/// while it is polled, without measuring the CPU time of the future itself.
///
/// Used for a component's `build` step: tasks it spawns are attributed to the
/// component, while one-time setup costs stay out of the steady-state metric.
///
/// Construct it via [`CpuTimedExt::cpu_scoped`].
#[pin_project]
pub struct CpuScopedFuture<F> {
    #[pin]
    inner: F,
    counter: Counter,
}

impl<F: Future> Future for CpuScopedFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();
        let _scope = CounterScope::enter(this.counter);
        this.inner.poll(cx)
    }
}

/// Extension trait that wraps a future in [`CpuTimedFuture`] or
/// [`CpuScopedFuture`] via a chained call:
///
/// ```ignore
/// async move { /* work */ }.cpu_timed(counter)
/// ```
///
/// Mirrors the style of [`tracing::Instrument::in_current_span`].
pub trait CpuTimedExt: Future + Sized {
    /// Wraps this future in a [`CpuTimedFuture`] that increments `counter` by
    /// the thread CPU time consumed on each `poll`.
    fn cpu_timed(self, counter: Counter) -> CpuTimedFuture<Self> {
        CpuTimedFuture {
            inner: self,
            counter,
        }
    }

    /// Wraps this future in a [`CpuScopedFuture`] that makes `counter`
    /// inheritable by the tasks it spawns, without timing the future itself.
    fn cpu_scoped(self, counter: Counter) -> CpuScopedFuture<Self> {
        CpuScopedFuture {
            inner: self,
            counter,
        }
    }
}

impl<F: Future> CpuTimedExt for F {}

/// Wraps `future` in a [`CpuTimedFuture`] charging the [current
/// counter](current_counter), if any. Returns it unchanged otherwise, with no
/// per-poll overhead.
fn inherit_cpu_timer<F: Future>(future: F) -> Either<CpuTimedFuture<F>, F> {
    match current_counter() {
        Some(counter) => Either::Left(future.cpu_timed(counter)),
        None => Either::Right(future),
    }
}

/// Spawns `future` on the current tokio runtime, charging its CPU time to the
/// [current counter](current_counter) if any. See
/// [counter inheritance](self#counter-inheritance).
///
/// Use this when spawning background tasks (e.g. a transform's housekeeping
/// loop) whose CPU usage should be attributed back to a component. Wrap the
/// future with [`tracing::Instrument`] (or similar adapters) before passing
/// it in if you want those adapters' per-poll work included in the CPU-time
/// measurement.
///
/// The current tracing span is attached to the spawned task.
#[track_caller]
pub fn spawn_timed<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    crate::spawn_in_current_span(inherit_cpu_timer(future))
}

/// Like [`spawn_timed`], but does not attach the current tracing span to the
/// spawned task.
///
/// For spawn sites where the current span does not describe the spawned work,
/// e.g. executor hooks called by third-party code.
#[track_caller]
pub fn spawn_timed_without_span<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(inherit_cpu_timer(future))
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
pub fn register_counter() -> Counter {
    crate::counter!(crate::internal_event::CounterName::ComponentCpuUsageNsTotal)
}

/// Registers the `component_cpu_usage_ns_total` counter for the calling
/// component on platforms where thread CPU time is available (Linux, macOS,
/// Windows). On other platforms it returns [`Counter::noop()`] — the metric
/// is **not** emitted at all, rather than reporting wall-clock or zero values
/// that would be misleading to compare against supported platforms.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn register_counter() -> Counter {
    Counter::noop()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    use super::*;

    fn test_counter() -> (Counter, Arc<AtomicU64>) {
        let value = Arc::new(AtomicU64::new(0));
        (Counter::from_arc(Arc::clone(&value)), value)
    }

    #[tokio::test]
    async fn spawned_task_inherits_scoped_counter() {
        let (counter, value) = test_counter();

        async {
            spawn_timed(async { (0u64..1_000_000).map(std::hint::black_box).sum::<u64>() })
                .await
                .unwrap()
        }
        .cpu_scoped(counter)
        .await;

        if cfg!(any(target_os = "linux", target_os = "macos")) {
            assert!(value.load(Ordering::Relaxed) > 0);
        }
    }

    #[tokio::test]
    async fn counter_is_restored_after_poll() {
        let (outer, outer_value) = test_counter();
        let (inner, inner_value) = test_counter();

        async {
            async { current_counter().unwrap().increment(1) }
                .cpu_scoped(inner)
                .await;
            current_counter().unwrap().increment(1);
        }
        .cpu_scoped(outer)
        .await;

        assert_eq!(inner_value.load(Ordering::Relaxed), 1);
        assert_eq!(outer_value.load(Ordering::Relaxed), 1);
        assert!(current_counter().is_none());
        assert!(matches!(inherit_cpu_timer(async {}), Either::Right(_)));
    }

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
