use std::{
    collections::HashMap,
    pin::Pin,
    task::{ready, Context, Poll},
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use futures::{Stream, StreamExt};
use metrics::Gauge;
use pin_project::pin_project;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{id::ComponentKey, shutdown::ShutdownSignal};

use crate::stats;

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
    pub(crate) fn new(gauge: Gauge) -> Self {
        Self {
            overall_start: Instant::now(),
            span_start: Instant::now(),
            waiting: false,
            total_wait: Duration::new(0, 0),
            ewma: stats::Ewma::new(0.9),
            gauge,
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
    pub(crate) fn stop_wait(&mut self, at: Instant) -> Instant {
        if self.waiting {
            let now = self.end_span(at);
            self.waiting = false;
            now
        } else {
            at
        }
    }

    /// Meant to be called on a regular interval, this method calculates wait
    /// ratio since the last time it was called and reports the resulting
    /// utilization average.
    pub(crate) fn report(&mut self) {
        // End the current span so it can be accounted for, but do not change
        // whether or not we're in the waiting state. This way the next span
        // inherits the correct status.
        let now = self.end_span(Instant::now());

        let total_duration = now.duration_since(self.overall_start);
        let wait_ratio = self.total_wait.as_secs_f64() / total_duration.as_secs_f64();
        let utilization = 1.0 - wait_ratio;

        self.ewma.update(utilization);
        let avg = self.ewma.average().unwrap_or(f64::NAN);
        debug!(utilization = %avg);
        self.gauge.set(avg);

        // Reset overall statistics for the next reporting period.
        self.overall_start = self.span_start;
        self.total_wait = Duration::new(0, 0);
    }

    fn end_span(&mut self, at: Instant) -> Instant {
        if self.waiting {
            self.total_wait += at - self.span_start;
        }
        self.span_start = at;
        self.span_start
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
            intervals: IntervalStream::new(interval(Duration::from_secs(5))),
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
        self.timers.insert(key.clone(), Timer::new(gauge));
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
