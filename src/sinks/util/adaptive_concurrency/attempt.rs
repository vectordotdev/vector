//! Per-attempt reporting for the adaptive concurrency controller.
//!
//! The concurrency limiter wraps the retry layer, so the only outcome it sees for a request is the
//! outcome of the last attempt, timed from the start of the first. This service sits on the other
//! side of the retry layer, where one call is one attempt, and reports each attempt to the
//! controller.

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
    time::Instant,
};

use pin_project::pin_project;
use tower::Service;

use super::{controller::Controller, instant_now};
use crate::sinks::util::retries::RetryLogic;

/// Reports the outcome and round-trip time of each request attempt to the controller it shares
/// with an [`super::AdaptiveConcurrencyLimit`].
pub struct MeasureAttempt<S, L> {
    inner: S,
    controller: Arc<Controller<L>>,
}

impl<S, L> MeasureAttempt<S, L> {
    pub(crate) const fn new(inner: S, controller: Arc<Controller<L>>) -> Self {
        Self { inner, controller }
    }
}

impl<S, L, Request> Service<Request> for MeasureAttempt<S, L>
where
    S: Service<Request, Error = crate::Error>,
    L: RetryLogic<Response = S::Response>,
{
    type Response = S::Response;
    type Error = crate::Error;
    type Future = MeasureAttemptFuture<S::Future, L>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        MeasureAttemptFuture {
            inner: self.inner.call(request),
            controller: Arc::clone(&self.controller),
            start: instant_now(),
        }
    }
}

impl<S: Clone, L> Clone for MeasureAttempt<S, L> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            controller: Arc::clone(&self.controller),
        }
    }
}

#[pin_project]
pub struct MeasureAttemptFuture<F, L> {
    #[pin]
    inner: F,
    controller: Arc<Controller<L>>,
    start: Instant,
}

impl<F, L> Future for MeasureAttemptFuture<F, L>
where
    F: Future<Output = Result<L::Response, crate::Error>>,
    L: RetryLogic,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let output = ready!(this.inner.poll(cx));
        this.controller.record_attempt(*this.start, &output);
        Poll::Ready(output)
    }
}
