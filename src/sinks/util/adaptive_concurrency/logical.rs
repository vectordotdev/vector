//! Limit whole logical requests, including the time they spend between retry attempts.

use std::{
    future::Future,
    mem,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};

use pin_project::pin_project;
use tokio::sync::OwnedSemaphorePermit;
use tower::{Layer, Service};

use super::{semaphore::ShrinkableSemaphore, service::State};

/// Layer for [`LogicalConcurrencyLimit`], sharing the semaphore of an adaptive limiter.
#[derive(Clone)]
pub(crate) struct LogicalConcurrencyLimitLayer {
    semaphore: Arc<ShrinkableSemaphore>,
}

impl LogicalConcurrencyLimitLayer {
    pub(super) const fn new(semaphore: Arc<ShrinkableSemaphore>) -> Self {
        Self { semaphore }
    }
}

impl<S> Layer<S> for LogicalConcurrencyLimitLayer {
    type Service = LogicalConcurrencyLimit<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LogicalConcurrencyLimit {
            inner,
            semaphore: Arc::clone(&self.semaphore),
            state: State::Empty,
        }
    }
}

/// Holds a permit for the whole life of a request, so a request backing off between retry
/// attempts still counts. The permit count follows the adaptive limiter's current limit.
pub struct LogicalConcurrencyLimit<S> {
    inner: S,
    semaphore: Arc<ShrinkableSemaphore>,
    state: State,
}

impl<S> LogicalConcurrencyLimit<S> {
    #[cfg(test)]
    pub(crate) const fn get_ref(&self) -> &S {
        &self.inner
    }
}

impl<S, Request> Service<Request> for LogicalConcurrencyLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LogicalResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match self.state {
                State::Ready(_) => return self.inner.poll_ready(cx),
                State::Waiting(ref mut fut) => {
                    tokio::pin!(fut);
                    let permit = ready!(fut.poll(cx));
                    State::Ready(permit)
                }
                State::Empty => State::Waiting(Box::pin(Arc::clone(&self.semaphore).acquire())),
            };
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let State::Ready(permit) = mem::replace(&mut self.state, State::Empty) else {
            panic!("Maximum requests in-flight; poll_ready must be called first");
        };
        LogicalResponseFuture {
            inner: self.inner.call(request),
            _permit: permit,
        }
    }
}

impl<S: Clone> Clone for LogicalConcurrencyLimit<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            semaphore: Arc::clone(&self.semaphore),
            state: State::Empty,
        }
    }
}

/// Future for [`LogicalConcurrencyLimit`], releasing its permit when dropped.
#[pin_project]
pub struct LogicalResponseFuture<F> {
    #[pin]
    inner: F,
    _permit: OwnedSemaphorePermit,
}

impl<F: Future> Future for LogicalResponseFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
