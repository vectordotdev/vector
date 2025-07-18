use std::{
    future::Future,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{stream::FuturesUnordered, Stream};
use pin_project::pin_project;

/// A set of futures which may complete in any order, and results are returned as a count of ready
/// futures. This is primarily useful for when we need to track that the futures have finished but
/// do not need to use their actual result value (ie `Output = ()`).
///
/// While callers could poll `FuturesUnordered` directly, only one result can be grabbed at a
/// time. As well, while the `ready_chunks` helper is available from `futures_util`, it uses an
/// internally fused stream, meaning that it cannot be used with `FuturesUnordered` as the first
/// `None` result from polling `FuturesUnordered` "fuses" all future polls of `ReadyChunks`,
/// effectively causing it to return no further items.
///
/// `FuturesUnorderedCount` takes the best of both worlds and combines the batching with the
/// unordered futures polling so that it can be used in a more straightforward way from user code.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub(crate) struct FuturesUnorderedCount<F: Future> {
    #[pin]
    futures: FuturesUnordered<F>,
    items: usize,
}

impl<F: Future> FuturesUnorderedCount<F> {
    /// Constructs a new, empty `FuturesUnorderedCount`.
    ///
    /// The returned `FuturesUnorderedCount` does not contain any futures. In this state,
    /// `FuturesUnorderedCount::poll_next` will return `Poll::Ready(None)`.
    pub(crate) fn new() -> Self {
        Self {
            futures: FuturesUnordered::new(),
            items: 0,
        }
    }

    /// Pushes a new future into the set.
    ///
    /// Callers must poll this stream in order to drive the underlying futures that have been stored.
    pub(crate) fn push(&mut self, fut: F) {
        self.futures.push(fut);
    }

    /// Returns `true` if the set contains no futures.
    pub(crate) fn is_empty(&self) -> bool {
        self.futures.is_empty()
    }

    /// Returns the number of futures contained in the set.
    ///
    /// This represents the total number of in-flight futures.
    pub(crate) fn len(&self) -> usize {
        self.futures.len()
    }
}

impl<F: Future> Stream for FuturesUnorderedCount<F> {
    type Item = usize;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.futures.as_mut().poll_next(cx) {
                // The underlying `FuturesUnordered` has no (more) available results, so if we have
                // anything, return it, otherwise, indicate that we're pending as well.
                Poll::Pending => {
                    return if *this.items == 0 {
                        Poll::Pending
                    } else {
                        Poll::Ready(Some(mem::take(this.items)))
                    }
                }

                // We got a future result, so bump the counter.
                Poll::Ready(Some(_item)) => *this.items += 1,

                // We have no pending futures, so simply return whatever have have stored, if
                // anything, or `None`.
                Poll::Ready(None) => {
                    let last = (*this.items > 0).then(|| mem::take(this.items));
                    return Poll::Ready(last);
                }
            }
        }
    }
}
