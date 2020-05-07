//! Future types
//!
use futures::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::OwnedSemaphorePermit;

/// Future for the `ConcurrencyLimit` service.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<T> {
    #[pin]
    inner: T,
    // Keep this around so that it is dropped when the future completes
    _permit: OwnedSemaphorePermit,
}

impl<T> ResponseFuture<T> {
    pub(super) fn new(inner: T, _permit: OwnedSemaphorePermit) -> ResponseFuture<T> {
        ResponseFuture { inner, _permit }
    }
}

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
{
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(ready!(self.project().inner.poll(cx)))
    }
}
