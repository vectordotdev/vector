use std::{
    future::Future,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{stream::FuturesUnordered, Stream};
use pin_project::pin_project;

/// A set of futures which may complete in any order, and results are returned in chunks.
///
/// While callers could poll `FuturesUnordered` directly, only one result can be grabbed at a
/// time. As well, while the `ready_chunks` helper is available from `futures_util`, it uses an
/// internally fused stream, meaning that it cannot be used with `FuturesUnordered` as the first
/// `None` result from polling `FuturesUnordered` "fuses" all future polls of `ReadyChunks`,
/// effectively causing it to return no further items.
///
/// `FuturesUnorderedChunked` takes the best of both worlds and combines the batching with the
/// unordered futures polling so that it can be used in a more straightforward way from user code.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct FuturesUnorderedChunked<F: Future> {
    #[pin]
    futures: FuturesUnordered<F>,
    items: Vec<F::Output>,
    chunk_size: usize,
}

impl<F: Future> FuturesUnorderedChunked<F> {
    /// Constructs a new, empty `FuturesUnorderedChunked`.
    ///
    /// The returned `FuturesUnorderedChunked` does not contain any futures. In this state, `FuturesUnordered
    /// Chunked::poll_next` will return `Poll::Ready(None)`.
    ///
    /// # Panics
    ///
    /// Will panic if `chunk_size` is zero.
    pub fn new(chunk_size: usize) -> Self {
        assert!(chunk_size > 0);

        Self {
            futures: FuturesUnordered::new(),
            items: Vec::with_capacity(chunk_size),
            chunk_size,
        }
    }

    /// Pushes a new future into the set.
    ///
    /// Callers must poll this stream in order to drive the underlying futures that have been stored.
    pub fn push(&mut self, fut: F) {
        self.futures.push(fut);
    }

    /// Returns `true` if the set contains no futures.
    pub fn is_empty(&self) -> bool {
        self.futures.is_empty()
    }

    /// Returns the number of futures contained in the set.
    ///
    /// This represents the total number of in-flight futures.
    pub fn len(&self) -> usize {
        self.futures.len()
    }
}

impl<F: Future> Stream for FuturesUnorderedChunked<F> {
    type Item = Vec<F::Output>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.futures.as_mut().poll_next(cx) {
                // The underlying `FuturesUnordered` has no (more) available results, so if we have
                // anything, return it, otherwise, indicate that we're pending as well.
                Poll::Pending => {
                    return if this.items.is_empty() {
                        Poll::Pending
                    } else {
                        Poll::Ready(Some(mem::replace(
                            this.items,
                            Vec::with_capacity(*this.chunk_size),
                        )))
                    }
                }

                // We got a future result, so store it.  Do the usual dance of returning what we
                // have if we've hit the chunk size.
                Poll::Ready(Some(item)) => {
                    this.items.push(item);
                    if this.items.len() >= *this.chunk_size {
                        return Poll::Ready(Some(mem::replace(
                            this.items,
                            Vec::with_capacity(*this.chunk_size),
                        )));
                    }
                }

                // We have no pending futures, so simply return whatever have have stored, if
                // anything, or `None`.
                Poll::Ready(None) => {
                    let last = if this.items.is_empty() {
                        None
                    } else {
                        let full_buf = mem::take(this.items);
                        Some(full_buf)
                    };

                    return Poll::Ready(last);
                }
            }
        }
    }
}
