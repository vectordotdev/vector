use std::pin::Pin;

use futures::{
    task::{Context, Poll},
    {Stream, StreamExt},
};

use crate::event::{EventArray, EventContainer};

const DEFAULT_CAPACITY: usize = 4096;

/// A stream combinator aimed at improving the performance of event streams under load.
///
/// This is similar in spirit to `StreamExt::ready_chunks`, but built specifically `EventArray`.
/// The more general `FoldReady` is left as an exercise to the reader.
pub struct ReadyArrays<T> {
    inner: T,
    enqueued: Vec<EventArray>,
    enqueued_size: usize,
    enqueued_limit: usize,
}

impl<T> ReadyArrays<T>
where
    T: Stream<Item = EventArray> + Unpin,
{
    /// Create a new `ReadyArrays` by wrapping an event array stream.
    pub fn new(inner: T) -> Self {
        Self::with_capacity(inner, DEFAULT_CAPACITY)
    }

    /// Create a new `ReadyArrays` with a specified capacity.
    ///
    /// The specified capacity is a soft limit, and chunks may be returned that contain more than
    /// that number of items.
    pub fn with_capacity(inner: T, cap: usize) -> Self {
        Self {
            inner,
            enqueued: Vec::with_capacity(cap),
            enqueued_size: 0,
            enqueued_limit: cap,
        }
    }

    /// Returns a reference to the underlying stream.
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the underlying stream.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    fn flush(&mut self) -> Vec<EventArray> {
        let arrays = std::mem::take(&mut self.enqueued);
        let size = self.enqueued_size;
        self.enqueued_size = 0;
        arrays
    }
}

impl<T> Stream for ReadyArrays<T>
where
    T: Stream<Item = EventArray> + Unpin,
{
    type Item = Vec<EventArray>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(array)) => {
                    self.enqueued.push(array);
                    self.enqueued_size += array.len();
                    if self.enqueued_size >= self.enqueued_limit {
                        return Poll::Ready(Some(self.flush()));
                    }
                }
                Poll::Ready(None) => {
                    if !self.enqueued.is_empty() {
                        return Poll::Ready(Some(self.flush()));
                    } else {
                        return Poll::Ready(None);
                    }
                }
                Poll::Pending => {
                    if !self.enqueued.is_empty() {
                        return Poll::Ready(Some(self.flush()));
                    } else {
                        return Poll::Pending;
                    }
                }
            }
        }
    }
}
