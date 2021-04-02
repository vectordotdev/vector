use crate::{stats, Event};
use async_stream::stream;
use futures::{Stream, StreamExt};
use std::time::{Duration, Instant};

/// Wrap a stream to emit stats about utilization. This is designed for use with the input channels
/// of transform and sinks components, and measures the amount of time that the stream is waiting
/// for input from upstream. We make the simplifying assumption that this wait time is when the
/// component is idle and the rest of the time it is doing useful work. This is more true for sinks
/// than transforms, which can be blocked by downstream components, but with knowledge of the
/// config the data is still useful.
pub fn wrap(inner: impl Stream<Item = Event>) -> impl Stream<Item = Event> {
    let mut timer = Timer::new();
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    stream! {
        tokio::pin!(inner);
        loop {
            timer.start_wait();
            let value = tokio::select! {
                value = inner.next() => {
                    timer.stop_wait();
                    value
                },
                _ = interval.tick() => {
                    timer.report();
                    continue
                }
            };
            if let Some(value) = value {
                yield value
            } else {
                break
            }
        }
    }
}

struct Timer {
    overall_start: Instant,
    span_start: Instant,
    waiting: bool,
    total_wait: Duration,
    ewma: stats::Ewma,
}

/// A simple, specialized timer for tracking spans of waiting vs not-waiting time and reporting
/// a smoothed estimate of utilization.
///
/// This implementation uses the idea of spans and reporting periods. Spans are a period of time
/// spent entirely in one state, aligning with state transitions but potentially more granular.
/// Reporting periods are expected to be of uniform length and used to aggregate span data into
/// time-weighted averages.
impl Timer {
    fn new() -> Self {
        Self {
            overall_start: Instant::now(),
            span_start: Instant::now(),
            waiting: false,
            total_wait: Duration::new(0, 0),
            ewma: stats::Ewma::new(0.9),
        }
    }

    /// Begin a new span representing time spent waiting
    fn start_wait(&mut self) {
        self.end_span();
        self.waiting = true;
    }

    /// Complete the current waiting span and begin a non-waiting span
    fn stop_wait(&mut self) {
        assert!(self.waiting);

        self.end_span();
        self.waiting = false;
    }

    /// Meant to be called on a regular interval, this method calculates wait ratio  since the last
    /// time it was called and reports the resulting utilization average.
    fn report(&mut self) {
        // End the current span so it can be accounted for, but do not change whether or not we're
        // in the waiting state. This way the next span inherits the correct status.
        self.end_span();

        let total_duration = self.overall_start.elapsed();
        let wait_ratio = self.total_wait.as_secs_f64() / total_duration.as_secs_f64();
        let utilization = 1.0 - wait_ratio;

        self.ewma.update(utilization);
        debug!(utilization = %self.ewma.average().unwrap_or(f64::NAN));

        // Reset overall statistics for the next reporting period.
        self.overall_start = self.span_start;
        self.total_wait = Duration::new(0, 0);
    }

    fn end_span(&mut self) {
        if self.waiting {
            self.total_wait += self.span_start.elapsed();
        }
        self.span_start = Instant::now();
    }
}
