use std::{
    future::Future,
    num::NonZeroUsize,
    panic,
    pin::Pin,
    task::{ready, Context, Poll},
};

use futures_util::{
    stream::{Fuse, FuturesOrdered},
    Stream, StreamExt,
};
use pin_project::pin_project;
use tokio::task::JoinHandle;

#[pin_project]
pub struct ConcurrentMap<St, T>
where
    St: Stream,
    T: Send + 'static,
{
    #[pin]
    stream: Fuse<St>,
    limit: Option<NonZeroUsize>,
    in_flight: FuturesOrdered<JoinHandle<T>>,
    f: Box<dyn Fn(St::Item) -> Pin<Box<dyn Future<Output = T> + Send + 'static>> + Send>,
}

impl<St, T> ConcurrentMap<St, T>
where
    St: Stream,
    T: Send + 'static,
{
    pub fn new<F>(stream: St, limit: Option<NonZeroUsize>, f: F) -> Self
    where
        F: Fn(St::Item) -> Pin<Box<dyn Future<Output = T> + Send + 'static>> + Send + 'static,
    {
        Self {
            stream: stream.fuse(),
            limit,
            in_flight: FuturesOrdered::new(),
            f: Box::new(f),
        }
    }
}

impl<St, T> Stream for ConcurrentMap<St, T>
where
    St: Stream,
    T: Send + 'static,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // The underlying stream is done, and we have no more in-flight futures.
        if this.stream.is_done() && this.in_flight.is_empty() {
            return Poll::Ready(None);
        }

        loop {
            let can_poll_stream = match this.limit {
                None => true,
                Some(limit) => this.in_flight.len() < limit.get(),
            };

            if can_poll_stream {
                match this.stream.as_mut().poll_next(cx) {
                    // Even if there's no items from the underlying stream, we still have the in-flight
                    // futures to check, so we don't return just yet.
                    Poll::Pending | Poll::Ready(None) => break,
                    Poll::Ready(Some(item)) => {
                        let fut = (this.f)(item);
                        let handle = tokio::spawn(fut);
                        this.in_flight.push_back(handle);
                    }
                }
            } else {
                // We're at our in-flight limit, so stop generating tasks for the moment.
                break;
            }
        }

        match ready!(this.in_flight.poll_next_unpin(cx)) {
            // Either nothing is in-flight, or nothing is ready.
            None => Poll::Pending,
            Some(result) => match result {
                Ok(item) => Poll::Ready(Some(item)),
                Err(e) => {
                    if let Ok(reason) = e.try_into_panic() {
                        // Resume the panic here on the calling task.
                        panic::resume_unwind(reason);
                    } else {
                        // The task was cancelled, which makes no sense, because _we_ hold the join
                        // handle. Only sensible thing to do is panic, because this is a bug.
                        panic!("concurrent map task cancelled outside of our control");
                    }
                }
            },
        }
    }
}
