use futures::ready;
use std::future::Future;
use std::mem::{drop, replace};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const EWMA_ALPHA: f64 = 0.5;

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
    dropping: usize,
    average_rtt: f64,
}

impl Controller {
    pub(super) fn new(max: usize, current: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(current)),
            max,
            inner: Arc::new(Mutex::new(Inner {
                current,
                dropping: 0,
                average_rtt: -1.0,
            })),
        }
    }

    /// Update the average RTT using EWMA, and return the new value.
    pub(super) fn update_rtt(&mut self, rtt: f64) -> f64 {
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");
        inner.average_rtt = match inner.average_rtt {
            avg if avg < 0.0 => rtt,
            avg => rtt * EWMA_ALPHA + avg * (1.0 - EWMA_ALPHA),
        };
        inner.average_rtt
    }

    /// Increase (additive) the current number of permits
    pub(super) fn expand(&mut self) {
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");
        if inner.current < self.max {
            self.semaphore.add_permits(1);
            inner.current += 1;
            inner.dropping = inner.dropping.saturating_sub(1);
        }
    }

    /// Decrease (multiplicative) the current concurrency
    pub(super) fn contract(&mut self) {
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");
        let new_current = (inner.current + 1) / 2;
        for _ in new_current..inner.current {
            match self.semaphore.try_acquire() {
                Ok(permit) => permit.forget(),
                Err(_) => inner.dropping += 1,
            }
        }
        inner.current = new_current;
    }

    pub(super) fn acquire(&self) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
        DroppingFuture {
            semaphore: self.semaphore.clone(),
            inner: self.inner.clone(),
            future: Box::pin(Arc::clone(&self.semaphore).acquire_owned()),
        }
    }
}

/// A future that accounts for the possibility of needing to forget some
/// number of permits before outputting a valid one.
struct DroppingFuture {
    semaphore: Arc<Semaphore>,
    inner: Arc<Mutex<Inner>>,
    future: Pin<Box<dyn Future<Output = OwnedSemaphorePermit> + Send + 'static>>,
}

impl Future for DroppingFuture {
    type Output = OwnedSemaphorePermit;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = self.inner.clone();
        let mut inner = inner.lock().expect("Controller mutex is poisoned");
        while inner.dropping > 0 {
            let permit = ready!(self.future.as_mut().poll(cx));
            inner.dropping -= 1;
            permit.forget();
            let future = Arc::clone(&self.semaphore).acquire_owned();
            replace(&mut self.future, Box::pin(future));
        }
        drop(inner);
        self.future.as_mut().poll(cx)
    }
}
