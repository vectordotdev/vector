use std::{cmp, num::NonZeroUsize, pin::Pin};

use futures::{
    task::{Context, Poll},
    {Stream, StreamExt},
};

use crate::event::{EventArray, EventContainer};

const ARRAY_BUFFER_DEFAULT_SIZE: usize = 16;

/// A stream combinator aimed at improving the performance of event streams under load.
///
/// This is similar in spirit to `StreamExt::ready_chunks`, but built specifically `EventArray`.
/// The more general `FoldReady` is left as an exercise to the reader.
pub struct ReadyArrays<T> {
    inner: T,
    /// Storage for ready `EventArray` instances. The size of `enqueued` is
    /// distinct from the `enqueued_size` field. While that field is in units of
    /// `Event`s this field is in units of `EventArray`. In the worst case where
    /// all `EventArray`s from the inner stream contain only a single `Event`
    /// the size of `enqueued` will grow to `enqueued_limit`.
    enqueued: Vec<EventArray>,
    /// Distinct from `enqueued.len()`, counts the number of total `Event`
    /// instances in all sub-arrays.
    enqueued_size: usize,
    /// Limit for the total number of `Event` instances, soft.
    enqueued_limit: usize,
}

impl<T> ReadyArrays<T>
where
    T: Stream<Item = EventArray> + Unpin,
{
    /// Create a new `ReadyArrays` with a specified capacity.
    ///
    /// The specified capacity is on the total number of `Event` instances
    /// enqueued here at one time. This is a soft limit. Chunks may be returned
    /// that contain more than that number of items.
    pub fn with_capacity(inner: T, capacity: NonZeroUsize) -> Self {
        Self {
            inner,
            enqueued: Vec::with_capacity(ARRAY_BUFFER_DEFAULT_SIZE),
            enqueued_size: 0,
            enqueued_limit: capacity.get(),
        }
    }

    fn flush(&mut self) -> Vec<EventArray> {
        // Size the next `enqueued` to the maximum of ARRAY_BUFFER_DEFAULT_SIZE
        // or the current length of `self.enqueued`. This means, in practice,
        // that we will always allocate at least the base size but possibly up
        // to `enqueued_limit` if the underlying stream passes singleton
        // EventArrays.
        let mut enqueued =
            Vec::with_capacity(cmp::max(self.enqueued.len(), ARRAY_BUFFER_DEFAULT_SIZE));
        std::mem::swap(&mut enqueued, &mut self.enqueued);
        self.enqueued_size = 0;
        enqueued
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
                    self.enqueued_size += array.len();
                    self.enqueued.push(array);
                    // NOTE pushing and then checking sizes here is what gives
                    // this struct a 'soft' limit guarantee. If we had a stash
                    // field we could make a hard limit, at the expense of
                    // sending undersized `Item`s. Slightly too big is fine.
                    if self.enqueued_size >= self.enqueued_limit {
                        return Poll::Ready(Some(self.flush()));
                    }
                }
                Poll::Ready(None) => {
                    // When the inner stream is empty flush everything we've got
                    // enqueued here. Next time we're polled we'll signal that
                    // we're complete too.
                    return Poll::Ready((!self.enqueued.is_empty()).then(|| self.flush()));
                }
                Poll::Pending => {
                    // When the inner stream signals pending flush everything
                    // we've got enqueued here. Next time we're polled we'll
                    // signal pending.
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
