//! Component utilization tracking and metrics.
//!
//! This module tracks how much time Vector components spend waiting for input versus actively processing data.
//! Utilization is calculated as a number between `0` and `1`, where `1` means fully utilized and `0` means idle.
//!
//! # Architecture
//!
//! - **Stream Wrapper**: `Utilization<S>` wraps component input streams and sends timing messages when polling.
//! - **Timer**: Tracks wait/active spans and calculates utilization via exponentially weighted moving average (EWMA).
//! - **Emitter**: Centralized `UtilizationEmitter` receives timing messages from all components and periodically reports metrics.
//!
//! # Message Flow
//!
//! 1. Component polls wrapped stream → `Utilization::poll_next()` sends `StartWait` message with timestamp
//! 2. Stream returns data → `Utilization::poll_next()` sends `StopWait` message with timestamp
//! 3. Messages queue in async channel and are processed by `UtilizationEmitter`
//! 4. Every `5` seconds, `Timer::report()` calculates utilization and updates the metric gauge
//!
//! # Delayed Message Handling
//!
//! Messages carry timestamps from when they were sent, but may be processed later due to channel queueing.
//! To prevent invalid utilization calculations, `Timer::end_span()` clamps timestamps to the current reporting
//! period boundary (`overall_start`), ensuring we only account for time within the current measurement window.

use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

#[cfg(not(test))]
use std::time::Instant;

#[cfg(test)]
use mock_instant::global::Instant;

#[cfg(debug_assertions)]
use std::sync::Arc;

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
    report_count: u32,
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
            report_count: 0,
            #[cfg(debug_assertions)]
            component_id,
        }
    }

    /// Begin a new span representing time spent waiting
    pub(crate) fn start_wait(&mut self, at: Instant) {
        if !self.waiting {
            self.end_span(at);
            self.waiting = true;
        }
    }

    /// Complete the current waiting span and begin a non-waiting span
    pub(crate) fn stop_wait(&mut self, at: Instant) {
        if self.waiting {
            self.end_span(at);
            self.waiting = false;
        }
    }

    /// Meant to be called on a regular interval, this method calculates wait
    /// ratio since the last time it was called and reports the resulting
    /// utilization average.
    pub(crate) fn report(&mut self) {
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

        self.overall_start = now;
        self.total_wait = Duration::new(0, 0);

        #[cfg(debug_assertions)]
        {
            // Note that changing the reporting interval would also affect the actual metric reporting frequency.
            // This check reduces debug log spamming.
            if self.report_count.is_multiple_of(5) {
                debug!(component_id = %self.component_id, utilization = %avg_rounded);
            }
            self.report_count = self.report_count.wrapping_add(1);
        }
    }

    fn end_span(&mut self, at: Instant) {
        // Clamp the timestamp to the current reporting period to handle delayed messages.
        // If 'at' is before overall_start (due to old timestamps from queued messages),
        // clamp it to overall_start to prevent accounting for time outside this period.
        let at_clamped = at.max(self.overall_start);

        if self.waiting {
            // Similarly, clamp span_start to ensure we don't count wait time from before this period
            let span_start_clamped = self.span_start.max(self.overall_start);
            self.total_wait += at_clamped.saturating_duration_since(span_start_clamped);
        }
        self.span_start = at_clamped;
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

pub(crate) struct UtilizationEmitter {
    timers: HashMap<ComponentKey, Timer>,
    timer_rx: Receiver<UtilizationTimerMessage>,
    timer_tx: Sender<UtilizationTimerMessage>,
    intervals: IntervalStream,
}

impl UtilizationEmitter {
    pub(crate) fn new() -> Self {
        let (timer_tx, timer_rx) = channel(4096);
        Self {
            timers: HashMap::default(),
            intervals: IntervalStream::new(interval(UTILIZATION_EMITTER_DURATION)),
            timer_tx,
            timer_rx,
        }
    }

    /// Adds a new component to this utilization metric emitter
    ///
    /// Returns a sender which can be used to send utilization information back to the emitter
    pub(crate) fn add_component(
        &mut self,
        key: ComponentKey,
        gauge: Gauge,
    ) -> UtilizationComponentSender {
        self.timers.insert(
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

    pub(crate) async fn run_utilization(&mut self, mut shutdown: ShutdownSignal) {
        loop {
            tokio::select! {
                message = self.timer_rx.recv() => {
                    match message {
                        Some(UtilizationTimerMessage::StartWait(key, start_time)) => {
                            self.timers.get_mut(&key).expect("Utilization timer missing for component").start_wait(start_time);
                        }
                        Some(UtilizationTimerMessage::StopWait(key, stop_time)) => {
                            self.timers.get_mut(&key).expect("Utilization timer missing for component").stop_wait(stop_time);
                        }
                        None => break,
                    }
                },

                Some(_) = self.intervals.next() => {
                    for timer in self.timers.values_mut() {
                        timer.report();
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

    #[test]
    #[serial]
    fn test_normal_utilization_within_bounds() {
        // Reset mock clock to ensure test isolation
        MockClock::set_time(Duration::ZERO);

        // Advance mock time to T=100 to avoid issues with time 0
        MockClock::advance(Duration::from_secs(100));

        let mut timer = Timer::new(
            metrics::gauge!("test_utilization"),
            #[cfg(debug_assertions)]
            "test_component".into(),
        );

        // Timer created at T=100. Advance 1 second and start waiting
        MockClock::advance(Duration::from_secs(1));
        timer.start_wait(Instant::now());

        // Advance 2 seconds while waiting (T=101 to T=103)
        MockClock::advance(Duration::from_secs(2));
        timer.stop_wait(Instant::now());

        // Advance 2 more seconds (not waiting), then report (T=103 to T=105)
        MockClock::advance(Duration::from_secs(2));
        timer.report();

        // total_wait = 2 seconds, total_duration = 5 seconds (T=100 to T=105)
        // wait_ratio = 2/5 = 0.4, utilization = 1.0 - 0.4 = 0.6
        let avg = timer.ewma.average().unwrap();
        assert!(
            avg >= 0.0 && avg <= 1.0,
            "Utilization {} is outside [0, 1]",
            avg
        );
        assert!(
            (avg - 0.6).abs() < 0.01,
            "Expected utilization ~0.6, got {}",
            avg
        );
    }

    #[test]
    #[serial]
    fn test_delayed_messages_can_cause_invalid_utilization() {
        // Reset mock clock to ensure test isolation
        MockClock::set_time(Duration::ZERO);

        // Start at T=100 to avoid time 0 issues
        MockClock::advance(Duration::from_secs(100));

        let mut timer = Timer::new(
            metrics::gauge!("test_utilization"),
            #[cfg(debug_assertions)]
            "test_component".into(),
        );

        // Timer created at T=100. Simulate that some time passes (to T=105)
        // and a report period completes, resetting overall_start
        MockClock::advance(Duration::from_secs(5));
        let now = Instant::now(); // T=105
        timer.overall_start = now; // Simulate report period reset

        // Now simulate delayed messages with old timestamps from before T=105
        // These represent messages sent at T=101, T=103, T=104 but processed after T=105
        let t1 = now - Duration::from_secs(4); // T=101
        let t3 = now - Duration::from_secs(2); // T=103
        let t4 = now - Duration::from_secs(1); // T=104

        // Process old messages - they should be clamped to overall_start (T=105)
        timer.start_wait(t1); // Should be clamped to T=105
        timer.stop_wait(t3); // Should be clamped to T=105 (no wait time added)
        timer.start_wait(t4); // Should be clamped to T=105

        // Advance 5 seconds and report (T=110)
        MockClock::advance(Duration::from_secs(5));
        timer.report();

        // With clamping: all old timestamps treated as T=105
        // So we waited from T=105 to T=110 = 5 seconds
        // total_duration = 5 seconds, total_wait = 5 seconds
        // wait_ratio = 1.0, utilization = 0.0
        let avg = timer.ewma.average().unwrap();
        assert!(
            avg >= 0.0 && avg <= 1.0,
            "Utilization {} is outside [0, 1]",
            avg
        );
        assert!(
            avg < 0.01,
            "Expected utilization near 0 (always waiting), got {}",
            avg
        );
    }

    #[test]
    #[serial]
    fn test_always_waiting_utilization() {
        // Reset mock clock to ensure test isolation
        MockClock::set_time(Duration::ZERO);

        // Start at T=100 to avoid time 0 issues
        MockClock::advance(Duration::from_secs(100));

        let mut timer = Timer::new(
            metrics::gauge!("test_utilization"),
            #[cfg(debug_assertions)]
            "test_component".into(),
        );

        // Timer created at T=100. Start waiting immediately
        timer.start_wait(Instant::now());

        // Advance 5 seconds while waiting (T=100 to T=105)
        MockClock::advance(Duration::from_secs(5));
        timer.report();

        // We waited the entire time: total_wait = 5s, total_duration = 5s
        // wait_ratio = 1.0, utilization = 0.0
        let avg = timer.ewma.average().unwrap();
        assert!(
            avg >= 0.0 && avg <= 1.0,
            "Utilization {} is outside [0, 1]",
            avg
        );
        assert!(
            avg < 0.01,
            "Expected utilization near 0 (always waiting), got {}",
            avg
        );
    }

    #[test]
    #[serial]
    fn test_never_waiting_utilization() {
        // Reset mock clock to ensure test isolation
        MockClock::set_time(Duration::ZERO);

        // Start at T=100 to avoid time 0 issues
        MockClock::advance(Duration::from_secs(100));

        let mut timer = Timer::new(
            metrics::gauge!("test_utilization"),
            #[cfg(debug_assertions)]
            "test_component".into(),
        );

        // Advance 5 seconds without waiting (T=100 to T=105)
        MockClock::advance(Duration::from_secs(5));
        timer.report();

        // Never waited: total_wait = 0, total_duration = 5s
        // wait_ratio = 0.0, utilization = 1.0
        let avg = timer.ewma.average().unwrap();
        assert!(
            avg >= 0.0 && avg <= 1.0,
            "Utilization {} is outside [0, 1]",
            avg
        );
        assert!(
            avg > 0.99,
            "Expected utilization near 1.0 (never waiting), got {}",
            avg
        );
    }
}
