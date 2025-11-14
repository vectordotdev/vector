use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
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
use tokio::{
    sync::mpsc::{Receiver, Sender, channel},
    time::interval,
};
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{id::ComponentKey, shutdown::ShutdownSignal};

use crate::stats;

const UTILIZATION_EMITTER_DURATION: Duration = Duration::from_secs(5);

#[pin_project]
pub(crate) struct Utilization<S> {
    intervals: IntervalStream,
    timer_tx: UtilizationComponentSender,
    component_key: ComponentKey,
    inner: S,
}

impl<S> Utilization<S> {
    /// Consumes this wrapper and returns the inner stream.
    ///
    /// This can't be constant because destructors can't be run in a const context, and we're
    /// discarding `IntervalStream`/`Timer` when we call this.
    #[allow(clippy::missing_const_for_fn)]
    pub(crate) fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> Stream for Utilization<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // The goal of this function is to measure the time between when the
        // caller requests the next Event from the stream and before one is
        // ready, with the side-effect of reporting every so often about how
        // long the wait gap is.
        //
        // This will just measure the time, while UtilizationEmitter collects
        // all the timers and emits utilization value periodically
        let this = self.project();
        this.timer_tx.try_send_start_wait();
        let _ = this.intervals.poll_next_unpin(cx);
        let result = ready!(this.inner.poll_next_unpin(cx));
        this.timer_tx.try_send_stop_wait();
        Poll::Ready(result)
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

#[derive(Debug)]
enum UtilizationTimerMessage {
    StartWait(ComponentKey, Instant),
    StopWait(ComponentKey, Instant),
}

pub(crate) struct UtilizationComponentSender {
    component_key: ComponentKey,
    timer_tx: Sender<UtilizationTimerMessage>,
}

impl UtilizationComponentSender {
    pub(crate) fn try_send_start_wait(&self) {
        if let Err(err) = self.timer_tx.try_send(UtilizationTimerMessage::StartWait(
            self.component_key.clone(),
            Instant::now(),
        )) {
            debug!(component_id = ?self.component_key, error = ?err, "Couldn't send utilization start wait message.");
        }
    }

    pub(crate) fn try_send_stop_wait(&self) {
        if let Err(err) = self.timer_tx.try_send(UtilizationTimerMessage::StopWait(
            self.component_key.clone(),
            Instant::now(),
        )) {
            debug!(component_id = ?self.component_key, error = ?err, "Couldn't send utilization stop wait message.");
        }
    }
}

/// Registry for components sending utilization data.
///
/// Cloning this is cheap and does not clone the underlying data.
#[derive(Clone)]
pub struct UtilizationRegistry {
    timers: Arc<Mutex<HashMap<ComponentKey, Timer>>>,
    timer_tx: Sender<UtilizationTimerMessage>,
}

impl UtilizationRegistry {
    /// Adds a new component to this utilization metric emitter
    ///
    /// Returns a sender which can be used to send utilization information back to the emitter
    pub(crate) fn add_component(
        &self,
        key: ComponentKey,
        gauge: Gauge,
    ) -> UtilizationComponentSender {
        self.timers.lock().expect("mutex poisoned").insert(
            key.clone(),
            Timer::new(
                gauge,
                #[cfg(debug_assertions)]
                key.id().into(),
            ),
        );
        UtilizationComponentSender {
            timer_tx: self.timer_tx.clone(),
            component_key: key,
        }
    }

    /// Removes a component from this utilization metric emitter
    pub(crate) fn remove_component(&self, key: &ComponentKey) {
        self.timers.lock().expect("mutex poisoned").remove(key);
    }
}

pub(crate) struct UtilizationEmitter {
    timers: Arc<Mutex<HashMap<ComponentKey, Timer>>>,
    timer_rx: Receiver<UtilizationTimerMessage>,
}

impl UtilizationEmitter {
    pub(crate) fn new() -> (Self, UtilizationRegistry) {
        let (timer_tx, timer_rx) = channel(4096);
        let timers = Arc::new(Mutex::new(HashMap::default()));
        (
            Self {
                timers: Arc::clone(&timers),
                timer_rx,
            },
            UtilizationRegistry { timers, timer_tx },
        )
    }

    pub(crate) async fn run_utilization(mut self, mut shutdown: ShutdownSignal) {
        let mut intervals = IntervalStream::new(interval(UTILIZATION_EMITTER_DURATION));
        loop {
            tokio::select! {
                message = self.timer_rx.recv() => {
                    match message {
                        Some(UtilizationTimerMessage::StartWait(key, start_time)) => {
                            // Timer could be removed in the registry while message is still in the queue
                            if let Some(timer) = self.timers.lock().expect("mutex poisoned").get_mut(&key) {
                                timer.start_wait(start_time);
                            }
                        }
                        Some(UtilizationTimerMessage::StopWait(key, stop_time)) => {
                            // Timer could be removed in the registry while message is still in the queue
                            if let Some(timer) = self.timers.lock().expect("mutex poisoned").get_mut(&key) {
                                timer.stop_wait(stop_time);
                            }
                        }
                        None => break,
                    }
                },

                Some(_) = intervals.next() => {
                    for timer in self.timers.lock().expect("mutex poisoned").values_mut() {
                        timer.update_utilization();
                    }
                },

                _ = &mut shutdown => {
                    break
                }
            }
        }
    }
}

/// Wrap a stream to emit stats about utilization. This is designed for use with
/// the input channels of transform and sinks components, and measures the
/// amount of time that the stream is waiting for input from upstream. We make
/// the simplifying assumption that this wait time is when the component is idle
/// and the rest of the time it is doing useful work. This is more true for
/// sinks than transforms, which can be blocked by downstream components, but
/// with knowledge of the config the data is still useful.
pub(crate) fn wrap<S>(
    timer_tx: UtilizationComponentSender,
    component_key: ComponentKey,
    inner: S,
) -> Utilization<S> {
    Utilization {
        intervals: IntervalStream::new(interval(Duration::from_secs(5))),
        timer_tx,
        component_key,
        inner,
    }
}

#[cfg(test)]
mod tests {
    use mock_instant::global::MockClock;
    use serial_test::serial;

    use super::*;

    /// Helper function to reset mock clock and create a timer at T=100
    fn setup_timer() -> Timer {
        // Set mock clock to T=100
        MockClock::set_time(Duration::from_secs(100));

        Timer::new(
            metrics::gauge!("test_utilization"),
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
}
