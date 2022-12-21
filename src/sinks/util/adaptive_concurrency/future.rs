//! Future types
//!
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
    time::Instant,
};

use pin_project::pin_project;
use tokio::sync::OwnedSemaphorePermit;

use super::{controller::Controller, instant_now};
use crate::sinks::util::retries::RetryLogic;

/// Future for the `AdaptiveConcurrencyLimit` service.
///
/// This future runs the inner future, which is used to collect the
/// response from the inner service, and then tells the controller to
/// adjust its measurements when that future is ready. It also owns the
/// semaphore permit that is used to control concurrency such that the
/// semaphore is returned when this future is dropped.
///
/// Note that this future must be awaited immediately (such as by
/// spawning it) to prevent extraneous delays from causing discrepancies
/// in the measurements.
#[pin_project]
pub struct ResponseFuture<F, L> {
    #[pin]
    inner: F,
    // Keep this around so that it is dropped when the future completes
    _permit: OwnedSemaphorePermit,
    controller: Arc<Controller<L>>,
    start: Instant,
}

impl<F, L> ResponseFuture<F, L> {
    pub(super) fn new(
        inner: F,
        _permit: OwnedSemaphorePermit,
        controller: Arc<Controller<L>>,
    ) -> Self {
        Self {
            inner,
            _permit,
            controller,
            start: instant_now(),
        }
    }
}

impl<F, L, E> Future for ResponseFuture<F, L>
where
    F: Future<Output = Result<L::Response, E>>,
    L: RetryLogic,
    E: Into<crate::Error>,
{
    type Output = Result<L::Response, crate::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let future = self.project();
        let output = ready!(future.inner.poll(cx)).map_err(Into::into);
        future.controller.adjust_to_response(*future.start, &output);
        Poll::Ready(output)
    }
}
