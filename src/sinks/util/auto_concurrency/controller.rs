use super::semaphore::ShrinkableSemaphore;
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
pub(super) struct Controller {
    semaphore: Arc<ShrinkableSemaphore>,
    max: usize,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    current: usize,
    past_rtt: EWMA,
    next_update: Instant,
    current_rtt: Mean,
}

impl Controller {
    pub(super) fn new(max: usize, current: usize) -> Self {
        Self {
            semaphore: Arc::new(ShrinkableSemaphore::new(current)),
            max,
            inner: Arc::new(Mutex::new(Inner {
                current,
                past_rtt: Default::default(),
                next_update: Instant::now(),
                current_rtt: Default::default(),
            })),
        }
    }

    pub(super) fn acquire(&self) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
        self.semaphore.clone().acquire()
    }

    pub(super) fn adjust_to_response(&self, start: Instant) {
        // Problems:
        // * adjusts for any response, does not differentiate
        let now = Instant::now();
        let rtt = now.saturating_duration_since(start).as_secs_f64();
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");

        let rtt = inner.current_rtt.update(rtt);
        let avg = inner.past_rtt.average();
        if avg > 0.0 && now >= inner.next_update {
            let threshold = avg * THRESHOLD_RATIO;
            if inner.current > 1 && rtt >= avg + threshold {
                // Decrease (multiplicative) the current concurrency
                let to_forget = inner.current / 2;
                self.semaphore.forget_permits(to_forget);
                inner.current -= to_forget;
            } else if inner.current < self.max && rtt <= avg {
                // Increase (additive) the current concurrency
                self.semaphore.add_permits(1);
                inner.current += 1;
            }

            let new_avg = inner.past_rtt.update(rtt);
            inner.next_update = now + Duration::from_secs_f64(new_avg);
        }

        inner.current_rtt.reset();
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
