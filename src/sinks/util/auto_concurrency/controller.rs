use futures::ready;
use std::future::Future;
use std::mem::{drop, replace};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const EWMA_ALPHA: f64 = 0.5;
const THRESHOLD_RATIO: f64 = 0.01;

/// Shared class for `tokio::sync::Semaphore` that manages adjusting the
/// semaphore size and other associated data.
#[derive(Clone, Debug)]
pub(super) struct Controller {
    semaphore: Arc<Semaphore>,
    max: usize,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    current: usize,
    to_forget: usize,
    measured_rtt: EWMA,
    next_update: Instant,
}

impl Controller {
    pub(super) fn new(max: usize, current: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(current)),
            max,
            inner: Arc::new(Mutex::new(Inner {
                current,
                to_forget: 0,
                measured_rtt: Default::default(),
                next_update: Instant::now(),
            })),
        }
    }

    pub(super) fn acquire(&self) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
        MaybeForgetFuture {
            semaphore: self.semaphore.clone(),
            inner: self.inner.clone(),
            future: Box::pin(Arc::clone(&self.semaphore).acquire_owned()),
        }
    }

    pub(super) fn adjust_to_response(&self, start: Instant) {
        // Problems:
        // * adjusts for any response, does not differentiate
        let now = Instant::now();
        let rtt = now.saturating_duration_since(start).as_secs_f64();
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");

        let avg = inner.measured_rtt.average();
        if avg > 0.0 && now >= inner.next_update {
            let threshold = avg * THRESHOLD_RATIO;
            if inner.current > 1 && rtt >= avg + threshold {
                // Decrease (multiplicative) the current concurrency
                let new_current = inner.current / 2;
                // Have to forget some permits from the semaphore but there
                // may not be enough available. If so, increase the count we
                // need to forget later and finish.
                for _ in new_current..inner.current {
                    match self.semaphore.try_acquire() {
                        Ok(permit) => permit.forget(),
                        Err(_) => inner.to_forget += 1,
                    }
                }
                inner.current = new_current;
            } else if inner.current < self.max && rtt <= avg {
                // Increase (additive) the current concurrency
                self.semaphore.add_permits(1);
                inner.current += 1;
                inner.to_forget = inner.to_forget.saturating_sub(1);
            }

            let new_avg = inner.measured_rtt.update(rtt);
            inner.next_update = now + Duration::from_secs_f64(new_avg);
        } else {
            inner.measured_rtt.update(rtt);
        }
    }
}

/// A future that accounts for the possibility of needing to forget some
/// number of permits before outputting a valid one.
struct MaybeForgetFuture {
    semaphore: Arc<Semaphore>,
    inner: Arc<Mutex<Inner>>,
    future: Pin<Box<dyn Future<Output = OwnedSemaphorePermit> + Send + 'static>>,
}

impl Future for MaybeForgetFuture {
    type Output = OwnedSemaphorePermit;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = self.inner.clone();
        let mut inner = inner.lock().expect("Controller mutex is poisoned");
        while inner.to_forget > 0 {
            let permit = ready!(self.future.as_mut().poll(cx));
            permit.forget();
            inner.to_forget -= 1;
            let future = Arc::clone(&self.semaphore).acquire_owned();
            replace(&mut self.future, Box::pin(future));
        }
        drop(inner);
        self.future.as_mut().poll(cx)
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
