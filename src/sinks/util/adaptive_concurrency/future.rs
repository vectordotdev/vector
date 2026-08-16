//! Future types
//!
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
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
    /// `None` when a `MeasureAttempt` reports each attempt to the controller, which times a
    /// single attempt rather than the whole retry sequence.
    start: Option<Instant>,
}

impl<F, L> ResponseFuture<F, L> {
    pub(super) fn new(
        inner: F,
        _permit: OwnedSemaphorePermit,
        controller: Arc<Controller<L>>,
        measure_call: bool,
    ) -> Self {
        Self {
            inner,
            _permit,
            controller,
            start: measure_call.then(instant_now),
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
        match future.start {
            Some(start) => future.controller.adjust_to_response(*start, &output),
            None => future.controller.adjust_to_completion(&output),
        }
        Poll::Ready(output)
    }
}
