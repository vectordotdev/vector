use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, ready},
    time::Duration,
};

#[cfg(not(test))]
use std::time::Instant;

#[cfg(test)]
use mock_instant::global::Instant;

use futures::{Stream, StreamExt};
use metrics::Gauge;
use pin_project::pin_project;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use vector_common::{id::ComponentKey, shutdown::ShutdownSignal, stats};

const UTILIZATION_EMITTER_DURATION: Duration = Duration::from_secs(5);

/// Input-side utilization wrapper.
///
/// Counts the time the wrapped stream spends returning `Poll::Pending` as time spent waiting for
/// upstream input. A poll that is immediately ready only costs one atomic load: the clock is read
/// and the shared timer is locked only when the component actually changes state.
#[pin_project]
pub struct Utilization<S> {
    timer: UtilizationComponentSender,
    inner: S,
}

impl<S> Utilization<S> {
    /// Wraps `inner` so that the time spent waiting on it is reported through `timer`.
    #[must_use]
    pub const fn new(timer: UtilizationComponentSender, inner: S) -> Self {
        Self { timer, inner }
    }

    /// Consumes this wrapper and returns the inner stream.
    ///
    /// This can't be constant because destructors can't be run in a const context, and we're
    /// discarding the timer handle when we call this.
    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> Stream for Utilization<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Poll first and only record a transition when the state changes: a component under
        // steady load is never pending here, so it never reads the clock.
        //
        // The waiting state lives in the shared timer rather than in this wrapper because other
        // wrappers of the same component (`OutputUtilization`, the sync transform runner) also
        // move it. Both calls below are a single atomic load when the state is already right.
        let this = self.project();
        match this.inner.poll_next_unpin(cx) {
            Poll::Ready(item) => {
                this.timer.stop_wait();
                Poll::Ready(item)
            }
            Poll::Pending => {
                this.timer.start_wait();
                Poll::Pending
            }
        }
    }
}

pub(crate) struct Timer {
    overall_start: Instant,
    span_start: Instant,
    waiting: bool,
    total_wait: Duration,
    ewma: stats::Ewma,
    gauge: Gauge,
    #[cfg(debug_assertions)]
    component_id: Arc<str>,
}

/// A simple, specialized timer for tracking spans of waiting vs not-waiting
/// time and reporting a smoothed estimate of utilization.
///
/// This implementation uses the idea of spans and reporting periods. Spans are
/// a period of time spent entirely in one state, aligning with state
/// transitions but potentially more granular.  Reporting periods are expected
/// to be of uniform length and used to aggregate span data into time-weighted
/// averages.
impl Timer {
    pub(crate) fn new(gauge: Gauge, #[cfg(debug_assertions)] component_id: Arc<str>) -> Self {
        Self {
            overall_start: Instant::now(),
            span_start: Instant::now(),
            waiting: false,
            total_wait: Duration::new(0, 0),
            ewma: stats::Ewma::new(0.9),
            gauge,
            #[cfg(debug_assertions)]
            component_id,
        }
    }

    /// Begin a new span representing time spent waiting
    pub(crate) fn start_wait(&mut self, at: Instant) {
        if !self.waiting {
            // Clamp start time in case of a late message
            self.end_span(at.max(self.overall_start));
            self.waiting = true;
        }
    }

    /// Complete the current waiting span and begin a non-waiting span
    pub(crate) fn stop_wait(&mut self, at: Instant) {
        if self.waiting {
            // Clamp stop time in case of a late message
            self.end_span(at.max(self.overall_start));
            self.waiting = false;
        }
    }

    /// Meant to be called on a regular interval, this method calculates wait
    /// ratio since the last time it was called and reports the resulting
    /// utilization average.
    pub(crate) fn update_utilization(&mut self) {
        // End the current span so it can be accounted for, but do not change
        // whether or not we're in the waiting state. This way the next span
        // inherits the correct status.
        let now = Instant::now();
        self.end_span(now);

        let total_duration = now.duration_since(self.overall_start);
        let wait_ratio = self.total_wait.as_secs_f64() / total_duration.as_secs_f64();
        let utilization = 1.0 - wait_ratio;

        self.ewma.update(utilization);
        let avg = self.ewma.average().unwrap_or(f64::NAN);
        let avg_rounded = (avg * 10000.0).round() / 10000.0; // 4 digit precision
        self.gauge.set(avg_rounded);

        // Reset overall statistics for the next reporting period.
        self.overall_start = now;
        self.total_wait = Duration::new(0, 0);

        #[cfg(debug_assertions)]
        debug!(component_id = %self.component_id, utilization = %avg_rounded, internal_log_rate_limit = false);
    }

    fn end_span(&mut self, at: Instant) {
        if self.waiting {
            // `at` can be before span start here, the result will be clamped to 0
            // because `duration_since` returns zero if `at` is before span start
            self.total_wait += at.duration_since(self.span_start);
        }
        self.span_start = at;
    }
}

/// Timer state shared between a component's wrappers and the emitter.
struct SharedTimer {
    /// Mirror of `timer.waiting`, read without locking so that redundant
    /// transitions (the common case) skip both the clock and the lock.
    waiting: AtomicBool,
    timer: parking_lot::Mutex<Timer>,
}

impl SharedTimer {
    fn new(timer: Timer) -> Self {
        Self {
            waiting: AtomicBool::new(timer.waiting),
            timer: parking_lot::Mutex::new(timer),
        }
    }

    fn set_waiting(&self, waiting: bool) {
        if self.waiting.load(Ordering::Relaxed) == waiting {
            return;
        }
        let mut timer = self.timer.lock();
        // Read the clock under the lock so that spans stay ordered with the
        // emitter's reporting periods.
        let now = Instant::now();
        if waiting {
            timer.start_wait(now);
        } else {
            timer.stop_wait(now);
        }
        self.waiting.store(waiting, Ordering::Relaxed);
    }
}

/// Handle a component uses to report when it starts and stops waiting.
///
/// Cloning it is cheap: clones share the same timer.
#[derive(Clone)]
pub struct UtilizationComponentSender {
    timer: Arc<SharedTimer>,
}

impl UtilizationComponentSender {
    /// Records that this component has started waiting.
    ///
    /// This is a single atomic load if the component is already waiting.
    pub fn start_wait(&self) {
        self.timer.set_waiting(true);
    }

    /// Records that this component has stopped waiting.
    ///
    /// This is a single atomic load if the component is not waiting.
    pub fn stop_wait(&self) {
        self.timer.set_waiting(false);
    }
}

type Timers = Arc<Mutex<HashMap<ComponentKey, Arc<SharedTimer>>>>;

/// Registry for components sending utilization data.
///
/// Cloning this is cheap and does not clone the underlying data.
#[derive(Clone)]
pub struct UtilizationRegistry {
    timers: Timers,
}

impl UtilizationRegistry {
    /// Adds a new component to this utilization metric emitter.
    ///
    /// Returns a handle the component uses to report its wait spans. If a component with the same
    /// key was already registered, it is replaced and handles to the old one stop being reported.
    ///
    /// # Panics
    ///
    /// Panics if the timer mutex is poisoned.
    #[must_use]
    pub fn add_component(&self, key: ComponentKey, gauge: Gauge) -> UtilizationComponentSender {
        let timer = Arc::new(SharedTimer::new(Timer::new(
            gauge,
            #[cfg(debug_assertions)]
            key.id().into(),
        )));
        self.timers
            .lock()
            .expect("mutex poisoned")
            .insert(key, Arc::clone(&timer));
        UtilizationComponentSender { timer }
    }

    /// Removes a component from this utilization metric emitter.
    ///
    /// # Panics
    ///
    /// Panics if the timer mutex is poisoned.
    pub fn remove_component(&self, key: &ComponentKey) {
        self.timers.lock().expect("mutex poisoned").remove(key);
    }
}

pub struct UtilizationEmitter {
    timers: Timers,
}

impl UtilizationEmitter {
    /// Creates an emitter and the registry components use to report wait spans.
    #[must_use]
    pub fn new() -> (Self, UtilizationRegistry) {
        let timers = Timers::default();
        (
            Self {
                timers: Arc::clone(&timers),
            },
            UtilizationRegistry { timers },
        )
    }

    /// Reports the utilization of every registered component.
    ///
    /// # Panics
    ///
    /// Panics if the timer mutex is poisoned.
    fn update_utilization(&self) {
        for timer in self.timers.lock().expect("mutex poisoned").values() {
            timer.timer.lock().update_utilization();
        }
    }

    /// Reports utilization periodically until `shutdown` resolves.
    ///
    /// # Panics
    ///
    /// Panics if the timer mutex is poisoned.
    pub async fn run_utilization(self, mut shutdown: ShutdownSignal) {
        let mut intervals = IntervalStream::new(interval(UTILIZATION_EMITTER_DURATION));
        loop {
            tokio::select! {
                Some(_) = intervals.next() => self.update_utilization(),
                _ = &mut shutdown => break,
            }
        }
    }
}

/// Output-side counterpart of [`Utilization`]. Wraps the output stream of a
/// task transform to track time spent waiting for downstream to accept items.
///
/// While [`Utilization`] measures idle time as "waiting for upstream input",
/// this wrapper measures the complementary case: after yielding an item, the
/// time until the consumer polls again is counted as downstream wait (idle).
/// Both wrappers share a single timer per component, and the idempotent
/// `start_wait`/`stop_wait` transitions ensure no double-counting.
pub struct OutputUtilization<S> {
    timer: UtilizationComponentSender,
    inner: S,
}

impl<S> Stream for OutputUtilization<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.timer.stop_wait();
        let result = ready!(this.inner.poll_next_unpin(cx));
        if result.is_some() {
            this.timer.start_wait();
        }
        Poll::Ready(result)
    }
}

impl<S> OutputUtilization<S> {
    /// Wrap a task transform stream to track time spent waiting for downstream
    /// to consume items. This is the output-side counterpart to
    /// [`Utilization::new`], designed for use with task transforms where the
    /// framework cannot otherwise detect downstream backpressure.
    #[must_use]
    pub const fn new(timer: UtilizationComponentSender, inner: S) -> Self {
        Self { timer, inner }
    }
}

#[cfg(test)]
mod tests {
    use mock_instant::global::MockClock;
    use serial_test::serial;
    use strum::IntoEnumIterator;
    use vector_common::{gauge, internal_event::GaugeName};

    use super::*;
    use crate::{
        event::{EventArray, LogEvent},
        transform::TaskTransform,
    };

    /// Helper function to reset mock clock and create a timer at T=100
    fn setup_timer() -> Timer {
        // Set mock clock to T=100
        MockClock::set_time(Duration::from_secs(100));

        Timer::new(
            gauge!(GaugeName::iter().next().unwrap()),
            #[cfg(debug_assertions)]
            "test_component".into(),
        )
    }

    const TOLERANCE: f64 = 0.01;

    /// Helper function to assert utilization is approximately equal to expected value
    /// and within valid bounds [0, 1]
    fn assert_approx_eq(actual: f64, expected: f64, description: &str) {
        assert!(
            (0.0..=1.0).contains(&actual),
            "Utilization {actual} is outside [0, 1]"
        );
        assert!(
            (actual - expected).abs() < TOLERANCE,
            "Expected utilization {description}, got {actual}"
        );
    }

    #[test]
    #[serial]
    fn test_utilization_in_bounds_on_late_start() {
        let mut timer = setup_timer();

        MockClock::advance(Duration::from_secs(5));

        timer.update_utilization();

        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 1.0, "near 1.0 (never waiting)");

        // Late message for start wait
        timer.start_wait(Instant::now() - Duration::from_secs(1));
        MockClock::advance(Duration::from_secs(5));

        timer.update_utilization();
        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 0.1, "~0.1");
    }

    #[test]
    #[serial]
    fn test_utilization_in_bounds_on_late_stop() {
        let mut timer = setup_timer();

        MockClock::advance(Duration::from_secs(5));

        timer.waiting = true;
        timer.update_utilization();

        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 0.0, "near 0 (always waiting)");

        // Late message for stop wait
        timer.stop_wait(Instant::now() - Duration::from_secs(4));
        MockClock::advance(Duration::from_secs(5));

        timer.update_utilization();
        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 0.9, "~0.9");
    }

    #[test]
    #[serial]
    fn test_normal_utilization_within_bounds() {
        let mut timer = setup_timer();

        // Timer created at T=100. Advance 1 second and start waiting
        MockClock::advance(Duration::from_secs(1));
        timer.start_wait(Instant::now());

        // Advance 2 seconds while waiting (T=101 to T=103)
        MockClock::advance(Duration::from_secs(2));
        timer.stop_wait(Instant::now());

        // Advance 2 more seconds (not waiting), then report (T=103 to T=105)
        MockClock::advance(Duration::from_secs(2));
        timer.update_utilization();

        // total_wait = 2 seconds, total_duration = 5 seconds (T=100 to T=105)
        // wait_ratio = 2/5 = 0.4, utilization = 1.0 - 0.4 = 0.6
        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 0.6, "~0.6");
    }

    #[test]
    #[serial]
    fn test_always_waiting_utilization() {
        let mut timer = setup_timer();

        // Timer created at T=100. Start waiting immediately
        timer.start_wait(Instant::now());

        // Advance 5 seconds while waiting (T=100 to T=105)
        MockClock::advance(Duration::from_secs(5));
        timer.update_utilization();

        // We waited the entire time: total_wait = 5s, total_duration = 5s
        // wait_ratio = 1.0, utilization = 0.0
        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 0.0, "near 0 (always waiting)");
    }

    #[test]
    #[serial]
    fn test_never_waiting_utilization() {
        let mut timer = setup_timer();

        // Advance 5 seconds without waiting (T=100 to T=105)
        MockClock::advance(Duration::from_secs(5));
        timer.update_utilization();

        // Never waited: total_wait = 0, total_duration = 5s
        // wait_ratio = 0.0, utilization = 1.0
        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, 1.0, "near 1.0 (never waiting)");
    }

    /// Mock task transform that passes events through unchanged, simulating
    /// a configurable processing delay by advancing `MockClock` per item.
    struct MockTaskTransform {
        processing_time: Duration,
    }

    impl TaskTransform<EventArray> for MockTaskTransform {
        fn transform(
            self: Box<Self>,
            task: Pin<Box<dyn Stream<Item = EventArray> + Send>>,
        ) -> Pin<Box<dyn Stream<Item = EventArray> + Send>> {
            let processing_time = self.processing_time;
            task.map(move |events| {
                MockClock::advance(processing_time);
                events
            })
            .boxed()
        }
    }

    /// End-to-end test exercising the Utilization (input) and
    /// `OutputUtilization` (output) stream wrappers with a mock `TaskTransform`,
    /// wired up the same way `build_task_transform` does in the builder.
    ///
    /// Pipeline: channel → Utilization(input) → `TaskTransform` → OutputUtilization(output)
    ///
    /// Timeline (10s):
    ///   T=100..103  waiting for input       (3s wait)
    ///   T=103..105  transform processing    (2s work)
    ///   T=105..108  blocked on downstream   (3s wait, even though input has data)
    ///   T=108..110  transform processing    (2s work)
    ///
    /// Expected utilization = 4/10 = 0.4
    #[tokio::test]
    #[serial]
    async fn test_task_transform_utilization_end_to_end() {
        use futures::SinkExt;
        use futures::channel::mpsc as futures_mpsc;

        MockClock::set_time(Duration::from_secs(100));

        let (emitter, registry) = UtilizationEmitter::new();
        let key = ComponentKey::from("test_transform");

        let sender = registry.add_component(key.clone(), gauge!(GaugeName::Utilization));
        let output_sender = sender.clone();

        // Upstream channel carrying EventArrays.
        let (mut input_tx, input_rx) = futures_mpsc::channel::<EventArray>(10);

        // Wire up the pipeline exactly like build_task_transform:
        //   Utilization(input) → transform.transform() → OutputUtilization(output)
        let input_wrapped = Utilization::new(sender, input_rx);
        let transform = Box::new(MockTaskTransform {
            processing_time: Duration::from_secs(2),
        });
        let transform_output = transform.transform(Box::pin(input_wrapped));
        let mut pipeline = OutputUtilization::new(output_sender, transform_output);

        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        // -- Phase 1: poll transform, no input available --
        // T=100: start_wait sent by input wrapper.
        assert!(Pin::new(&mut pipeline).poll_next(&mut cx).is_pending());
        MockClock::advance(Duration::from_secs(3));
        // T=103: 3s of input wait. Period: 0s work / 3s → util = 0.0.
        check_timer_utilization(&emitter, &key, 0.0, "0.0 (phase 1: all input wait)");

        // -- Phase 2: send input, poll pipeline → transform processes --
        input_tx
            .send(EventArray::from(LogEvent::default()))
            .await
            .unwrap();
        assert!(Pin::new(&mut pipeline).poll_next(&mut cx).is_ready());
        // T=105: MockClock advanced 2s inside transform. start_wait sent by
        // output wrapper. Period: 2s work / 0s wait → util = 1.0.
        // EWMA: 0.9×1.0 + 0.1×0.0 = 0.9
        check_timer_utilization(&emitter, &key, 0.9, "0.9 (phase 2: all work)");

        // -- Phase 3: send another event, simulate downstream blocking --
        // Data is available in the input channel, but the transform is not
        // polled so no progress is made: the time counts as downstream wait.
        input_tx
            .send(EventArray::from(LogEvent::default()))
            .await
            .unwrap();
        MockClock::advance(Duration::from_secs(3));
        // T=108: 3s downstream wait. Period: 0s work / 3s → util = 0.0.
        // EWMA: 0.9×0.0 + 0.1×0.9 = 0.09
        check_timer_utilization(
            &emitter,
            &key,
            0.09,
            "0.09 (phase 3: downstream wait despite buffered input)",
        );

        // -- Phase 4: poll pipeline, processes the queued event --
        assert!(Pin::new(&mut pipeline).poll_next(&mut cx).is_ready());
        // T=110: MockClock advanced 2s inside transform. Period: 2s work / 0s wait → util = 1.0.
        // EWMA: 0.9×1.0 + 0.1×0.09 ≈ 0.909
        check_timer_utilization(
            &emitter,
            &key,
            0.909,
            "0.909 (phase 4: queued event processed)",
        );
    }

    /// Run one reporting period for a single component and assert its current
    /// EWMA utilization.
    fn check_timer_utilization(
        emitter: &UtilizationEmitter,
        key: &ComponentKey,
        expected: f64,
        description: &str,
    ) {
        let timers = emitter.timers.lock().expect("mutex poisoned");
        let mut timer = timers.get(key).expect("timer should exist").timer.lock();
        timer.update_utilization();
        let avg = timer.ewma.average().unwrap();
        assert_approx_eq(avg, expected, description);
    }

    fn is_waiting(sender: &UtilizationComponentSender) -> bool {
        let waiting = sender.timer.waiting.load(Ordering::Relaxed);
        assert_eq!(
            waiting,
            sender.timer.timer.lock().waiting,
            "mirror out of sync"
        );
        waiting
    }

    /// A stream that is always ready must never be counted as waiting.
    #[test]
    #[serial]
    fn test_ready_stream_never_waits() {
        MockClock::set_time(Duration::from_secs(100));
        let (emitter, registry) = UtilizationEmitter::new();
        let key = ComponentKey::from("ready");
        let sender = registry.add_component(key.clone(), gauge!(GaugeName::Utilization));
        let mut stream = Utilization::new(sender.clone(), futures::stream::repeat(1));

        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        for _ in 0..1000 {
            assert!(Pin::new(&mut stream).poll_next(&mut cx).is_ready());
            assert!(!is_waiting(&sender));
        }
        MockClock::advance(Duration::from_secs(5));
        check_timer_utilization(&emitter, &key, 1.0, "1.0 (always ready)");
    }

    /// Pending polls start a wait span and the next ready poll ends it; repeated
    /// pending polls don't restart it.
    #[test]
    #[serial]
    fn test_pending_then_ready_records_wait() {
        MockClock::set_time(Duration::from_secs(100));
        let (emitter, registry) = UtilizationEmitter::new();
        let key = ComponentKey::from("pending");
        let sender = registry.add_component(key.clone(), gauge!(GaugeName::Utilization));
        let (tx, rx) = futures::channel::mpsc::unbounded::<u32>();
        let mut stream = Utilization::new(sender.clone(), rx);

        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        // T=100..104: pending, polled several times (spurious wakeups).
        for _ in 0..4 {
            assert!(Pin::new(&mut stream).poll_next(&mut cx).is_pending());
            assert!(is_waiting(&sender));
            MockClock::advance(Duration::from_secs(1));
        }
        // T=104: input arrives.
        tx.unbounded_send(1).unwrap();
        assert!(Pin::new(&mut stream).poll_next(&mut cx).is_ready());
        assert!(!is_waiting(&sender));
        // T=104..105: busy.
        MockClock::advance(Duration::from_secs(1));
        // 4s wait out of 5s → utilization 0.2.
        check_timer_utilization(&emitter, &key, 0.2, "0.2 (4s of 5s waiting)");
    }

    /// When another wrapper of the same component clears the waiting state
    /// while the input is still pending, the next pending poll must mark the
    /// component as waiting again.
    #[test]
    #[serial]
    fn test_pending_rearms_after_shared_stop() {
        MockClock::set_time(Duration::from_secs(100));
        let (emitter, registry) = UtilizationEmitter::new();
        let key = ComponentKey::from("rearm");
        let sender = registry.add_component(key.clone(), gauge!(GaugeName::Utilization));
        let (_tx, rx) = futures::channel::mpsc::unbounded::<u32>();
        let mut stream = Utilization::new(sender.clone(), rx);

        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        assert!(Pin::new(&mut stream).poll_next(&mut cx).is_pending());
        // E.g. `OutputUtilization` being polled again by downstream.
        sender.stop_wait();
        assert!(!is_waiting(&sender));
        assert!(Pin::new(&mut stream).poll_next(&mut cx).is_pending());
        assert!(is_waiting(&sender));

        MockClock::advance(Duration::from_secs(5));
        check_timer_utilization(&emitter, &key, 0.0, "0.0 (waiting on input)");
    }

    /// Removed components are no longer reported, and re-adding a key does not
    /// let handles to the old component affect the new one.
    #[test]
    #[serial]
    fn test_readd_component_isolates_old_handles() {
        MockClock::set_time(Duration::from_secs(100));
        let (emitter, registry) = UtilizationEmitter::new();
        let key = ComponentKey::from("reload");
        let old = registry.add_component(key.clone(), gauge!(GaugeName::Utilization));
        registry.remove_component(&key);
        assert!(emitter.timers.lock().unwrap().get(&key).is_none());

        let new = registry.add_component(key.clone(), gauge!(GaugeName::Utilization));
        old.start_wait();
        assert!(!is_waiting(&new));
        MockClock::advance(Duration::from_secs(5));
        check_timer_utilization(&emitter, &key, 1.0, "1.0 (old handle ignored)");
    }
}
