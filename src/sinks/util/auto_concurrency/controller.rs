use super::semaphore::ShrinkableSemaphore;
use crate::sinks::util::retries2::RetryLogic;
use std::cmp::max;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;

const EWMA_ALPHA: f64 = 0.5;
const THRESHOLD_RATIO: f64 = 0.01;

/// Shared class for `tokio::sync::Semaphore` that manages adjusting the
/// semaphore size and other associated data.
#[derive(Clone, Debug)]
pub(super) struct Controller<L> {
    semaphore: Arc<ShrinkableSemaphore>,
    max: usize,
    logic: L,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    current: usize,
    past_rtt: EWMA,
    next_update: Instant,
    current_rtt: Mean,
    had_back_pressure: bool,
}

impl<L> Controller<L> {
    pub(super) fn new(max: usize, logic: L, current: usize) -> Self {
        Self {
            semaphore: Arc::new(ShrinkableSemaphore::new(current)),
            max,
            logic,
            inner: Arc::new(Mutex::new(Inner {
                current,
                past_rtt: Default::default(),
                next_update: Instant::now(),
                current_rtt: Default::default(),
                had_back_pressure: false,
            })),
        }
    }

    pub(super) fn acquire(&self) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
        self.semaphore.clone().acquire()
    }

    fn adjust_to_back_pressure(&self, start: Instant, is_back_pressure: bool) {
        let now = Instant::now();
        let rtt = now.saturating_duration_since(start).as_secs_f64();
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");
        if is_back_pressure {
            inner.had_back_pressure = true;
        }

        let rtt = inner.current_rtt.update(rtt);
        let avg = inner.past_rtt.average();
        if avg > 0.0 && now >= inner.next_update {
            let threshold = avg * THRESHOLD_RATIO;

            // Back pressure responses, either explicit or implicit due
            // to increasing response times, trigger a decrease in
            // concurrency.
            if inner.current > 1 && (inner.had_back_pressure || rtt >= avg + threshold) {
                // Decrease (multiplicative) the current concurrency
                let to_forget = inner.current / 2;
                self.semaphore.forget_permits(to_forget);
                inner.current -= to_forget;
            }
            // Normal quick responses trigger an increase in concurrency.
            else if inner.current < self.max && !inner.had_back_pressure && rtt <= avg {
                // Increase (additive) the current concurrency
                self.semaphore.add_permits(1);
                inner.current += 1;
            }

            let new_avg = inner.past_rtt.update(rtt);
            inner.next_update = now + Duration::from_secs_f64(new_avg);
        }

        inner.had_back_pressure = false;
        inner.current_rtt.reset();
    }
}

impl<L> Controller<L>
where
    L: RetryLogic,
{
    pub(super) fn adjust_to_response(
        &self,
        start: Instant,
        response: &Result<L::Response, L::Error>,
    ) {
        let is_back_pressure = matches!(response, Err(r) if self.logic.is_retriable_error(r));
        self.adjust_to_back_pressure(start, is_back_pressure)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct EWMA {
    average: f64,
}

impl EWMA {
    fn average(&self) -> f64 {
        self.average
    }

    fn update(&mut self, point: f64) -> f64 {
        self.average = match self.average {
            avg if avg == 0.0 => point,
            avg => point * EWMA_ALPHA + avg * (1.0 - EWMA_ALPHA),
        };
        self.average
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Mean {
    sum: f64,
    count: usize,
}

impl Mean {
    fn update(&mut self, point: f64) -> f64 {
        self.sum += point;
        self.count += 1;
        // Return current average
        self.sum / max(self.count, 1) as f64
    }

    fn reset(&mut self) {
        self.sum = 0.0;
        self.count = 0;
    }
}
