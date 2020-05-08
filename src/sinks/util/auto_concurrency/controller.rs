use futures::ready;
use std::future::Future;
use std::mem::replace;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Wrapper class for `tokio::sync::Semaphore` that allows for easily
/// adjusting the number of permits up to a maximum and down to 1.
// FIXME: This will need a custom impl Clone later
#[derive(Clone, Debug)]
pub(super) struct Controller {
    semaphore: Arc<Semaphore>,
    max: usize,
    current: usize,
    dropping: usize,
}

impl Controller {
    pub(super) fn new(max: usize, current: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(current)),
            max,
            current,
            dropping: 0,
        }
    }

    /// Increase (additive) the current number of permits
    pub(super) fn increase(&mut self) {
        if self.current < self.max {
            self.semaphore.add_permits(1);
            self.current += 1;
            self.dropping = self.dropping.saturating_sub(1);
        }
    }

    /// Decrease (multiplicative) the current concurrency
    pub(super) fn decrease(&mut self) {
        let new_current = (self.current + 1) / 2;
        for _ in new_current..self.current {
            match self.semaphore.try_acquire() {
                Ok(permit) => permit.forget(),
                Err(_) => self.dropping += 1,
            }
        }
        self.current = new_current;
    }

    pub(super) fn acquire(
        &mut self,
    ) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
        DroppingFuture {
            semaphore: Arc::clone(&self.semaphore),
            dropping: replace(&mut self.dropping, 0),
            future: Box::pin(Arc::clone(&self.semaphore).acquire_owned()),
        }
    }
}

/// A future that accounts for the possibility of needing to forget some
/// number of permits before outputting a valid one.
struct DroppingFuture {
    semaphore: Arc<Semaphore>,
    dropping: usize,
    future: Pin<Box<dyn Future<Output = OwnedSemaphorePermit> + Send + 'static>>,
}

impl Future for DroppingFuture {
    type Output = OwnedSemaphorePermit;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while self.dropping > 0 {
            let permit = ready!(self.future.as_mut().poll(cx));
            self.dropping -= 1;
            permit.forget();
            let future = Arc::clone(&self.semaphore).acquire_owned();
            replace(&mut self.future, Box::pin(future));
        }
        self.future.as_mut().poll(cx)
    }
}
