use std::pin::Pin;

use futures::{
    task::{Context, Poll},
    {Stream, StreamExt},
};

/// A stream combinator aimed at improving the performance of decoder streams under load.
///
/// This is similar in spirit to `StreamExt::ready_chunks`, but built specifically for the
/// particular result tuple returned by decoding streams. The more general `FoldReady` is left as
/// an exercise to the reader.
pub struct ReadyFrames<T, U, E> {
    inner: T,
    enqueued: Vec<U>,
    enqueued_size: usize,
    error_slot: Option<E>,
}

impl<T, U, E> ReadyFrames<T, U, E>
where
    T: Stream<Item = Result<(U, usize), E>> + Unpin,
    U: Unpin,
    E: Unpin,
{
    /// Create a new `ReadyChunks` by wrapping a decoder stream, most commonly a `FramedRead`.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            enqueued: Vec::with_capacity(128),
            enqueued_size: 0,
            error_slot: None,
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

    fn flush(&mut self) -> (Vec<U>, usize) {
        let frames = std::mem::take(&mut self.enqueued);
        let size = self.enqueued_size;
        self.enqueued_size = 0;
        (frames, size)
    }
}

impl<T, U, E> Stream for ReadyFrames<T, U, E>
where
    T: Stream<Item = Result<(U, usize), E>> + Unpin,
    U: Unpin,
    E: Unpin,
{
    type Item = Result<(Vec<U>, usize), E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(error) = self.error_slot.take() {
            return Poll::Ready(Some(Err(error)));
        }

        loop {
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok((frame, size)))) => {
                    self.enqueued.push(frame);
                    self.enqueued_size += size;
                    if self.enqueued.len() >= 1024 {
                        return Poll::Ready(Some(Ok(self.flush())));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    if self.enqueued.is_empty() {
                        return Poll::Ready(Some(Err(error)));
                    } else {
                        self.error_slot = Some(error);
                        return Poll::Ready(Some(Ok(self.flush())));
                    }
                }
                Poll::Ready(None) => {
                    if !self.enqueued.is_empty() {
                        return Poll::Ready(Some(Ok(self.flush())));
                    } else {
                        return Poll::Ready(None);
                    }
                }
                Poll::Pending => {
                    if !self.enqueued.is_empty() {
                        return Poll::Ready(Some(Ok(self.flush())));
                    } else {
                        return Poll::Pending;
                    }
                }
            }
        }
    }
}
