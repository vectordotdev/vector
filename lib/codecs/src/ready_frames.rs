use std::pin::Pin;

use futures::{
    Stream, StreamExt,
    task::{Context, Poll},
};

const DEFAULT_CAPACITY: usize = 1024;

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
    enqueued_limit: usize,
    inner_pending: bool,
}

impl<T, U, E> ReadyFrames<T, U, E>
where
    T: Stream<Item = Result<(U, usize), E>> + Unpin,
    U: Unpin,
    E: Unpin,
{
    /// Create a new `ReadyChunks` by wrapping a decoder stream, most commonly a `FramedRead`.
    pub fn new(inner: T) -> Self {
        Self::with_capacity(inner, DEFAULT_CAPACITY)
    }

    /// Create a new `ReadyChunks` with a specified capacity by wrapping a decoder stream, most
    /// commonly a `FramedRead`.
    ///
    /// The specified capacity is a soft limit, and chunks may be returned that contain more than
    /// that number of items.
    pub fn with_capacity(inner: T, cap: usize) -> Self {
        Self {
            inner,
            enqueued: Vec::with_capacity(cap),
            enqueued_size: 0,
            error_slot: None,
            enqueued_limit: cap,
            inner_pending: false,
        }
    }

    /// Returns a reference to the underlying stream.
    pub const fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the underlying stream.
    pub const fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Returns `true` if the last poll ended because the underlying stream returned
    /// `Poll::Pending`.
    ///
    /// In that case the underlying stream had no more items ready, and it will only produce more
    /// after it wakes the task again. Otherwise (for example, when the last chunk was cut at the
    /// capacity), the underlying stream may still have items ready.
    pub const fn is_inner_pending(&self) -> bool {
        self.inner_pending
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
        self.inner_pending = false;

        if let Some(error) = self.error_slot.take() {
            return Poll::Ready(Some(Err(error)));
        }

        loop {
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok((frame, size)))) => {
                    self.enqueued.push(frame);
                    self.enqueued_size += size;
                    if self.enqueued.len() >= self.enqueued_limit {
                        return Poll::Ready(Some(Ok(self.flush())));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    if self.enqueued.is_empty() {
                        return Poll::Ready(Some(Err(error)));
                    }
                    self.error_slot = Some(error);
                    return Poll::Ready(Some(Ok(self.flush())));
                }
                Poll::Ready(None) => {
                    if self.enqueued.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(self.flush())));
                }
                Poll::Pending => {
                    self.inner_pending = true;
                    if self.enqueued.is_empty() {
                        return Poll::Pending;
                    }
                    return Poll::Ready(Some(Ok(self.flush())));
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use futures::{SinkExt, StreamExt, channel::mpsc, poll, task::Poll};

    use super::ReadyFrames;

    #[tokio::test]
    async fn idle_passthrough() {
        let (mut tx, rx) = mpsc::channel::<Result<(&str, usize), &str>>(5);
        let mut rf = ReadyFrames::with_capacity(rx, 2);

        assert_eq!(Poll::Pending, poll!(rf.next()));

        tx.send(Ok(("foo", 1))).await.unwrap();

        assert_eq!(Poll::Ready(Some(Ok((vec!["foo"], 1)))), poll!(rf.next()));
        assert_eq!(Poll::Pending, poll!(rf.next()));
    }

    #[tokio::test]
    async fn limits_to_capacity() {
        let (mut tx, rx) = mpsc::channel::<Result<(&str, usize), &str>>(5);
        let mut rf = ReadyFrames::with_capacity(rx, 2);

        tx.send(Ok(("foo", 2))).await.unwrap();
        tx.send(Ok(("bar", 3))).await.unwrap();

        assert_eq!(
            Poll::Ready(Some(Ok((vec!["foo", "bar"], 5)))),
            poll!(rf.next())
        );
        assert_eq!(Poll::Pending, poll!(rf.next()));

        tx.send(Ok(("foo", 4))).await.unwrap();
        tx.send(Ok(("bar", 5))).await.unwrap();
        tx.send(Ok(("baz", 6))).await.unwrap();

        assert_eq!(
            Poll::Ready(Some(Ok((vec!["foo", "bar"], 9)))),
            poll!(rf.next())
        );
        assert_eq!(Poll::Ready(Some(Ok((vec!["baz"], 6)))), poll!(rf.next()));
        assert_eq!(Poll::Pending, poll!(rf.next()));
    }

    #[tokio::test]
    async fn error_passing() {
        let (mut tx, rx) = mpsc::channel::<Result<(&str, usize), &str>>(5);
        let mut rf = ReadyFrames::with_capacity(rx, 2);

        tx.send(Err("oops")).await.unwrap();

        assert_eq!(Poll::Ready(Some(Err("oops"))), poll!(rf.next()));
        assert_eq!(Poll::Pending, poll!(rf.next()));

        tx.send(Ok(("foo", 7))).await.unwrap();
        tx.send(Err("oops")).await.unwrap();

        assert_eq!(Poll::Ready(Some(Ok((vec!["foo"], 7)))), poll!(rf.next()));
        assert_eq!(Poll::Ready(Some(Err("oops"))), poll!(rf.next()));
        assert_eq!(Poll::Pending, poll!(rf.next()));
    }

    #[tokio::test]
    async fn reports_when_inner_is_pending() {
        let (mut tx, rx) = mpsc::channel::<Result<(&str, usize), &str>>(5);
        let mut rf = ReadyFrames::with_capacity(rx, 2);
        assert!(!rf.is_inner_pending());

        assert_eq!(Poll::Pending, poll!(rf.next()));
        assert!(rf.is_inner_pending());

        // A chunk cut at the capacity leaves items in the inner stream.
        tx.send(Ok(("foo", 1))).await.unwrap();
        tx.send(Ok(("bar", 2))).await.unwrap();
        tx.send(Ok(("baz", 3))).await.unwrap();
        assert_eq!(
            Poll::Ready(Some(Ok((vec!["foo", "bar"], 3)))),
            poll!(rf.next())
        );
        assert!(!rf.is_inner_pending());

        // A chunk that ends because the inner stream has nothing more ready.
        assert_eq!(Poll::Ready(Some(Ok((vec!["baz"], 3)))), poll!(rf.next()));
        assert!(rf.is_inner_pending());

        // A chunk that ends at an error leaves the error to be returned next.
        tx.send(Ok(("foo", 4))).await.unwrap();
        tx.send(Err("oops")).await.unwrap();
        assert_eq!(Poll::Ready(Some(Ok((vec!["foo"], 4)))), poll!(rf.next()));
        assert!(!rf.is_inner_pending());
        assert_eq!(Poll::Ready(Some(Err("oops"))), poll!(rf.next()));
        assert!(!rf.is_inner_pending());
    }

    #[tokio::test]
    async fn closing() {
        let (mut tx, rx) = mpsc::channel::<Result<(&str, usize), &str>>(5);
        let mut rf = ReadyFrames::with_capacity(rx, 2);

        tx.send(Ok(("foo", 8))).await.unwrap();
        tx.send(Ok(("bar", 9))).await.unwrap();
        tx.send(Ok(("baz", 10))).await.unwrap();
        drop(tx);

        assert_eq!(
            Poll::Ready(Some(Ok((vec!["foo", "bar"], 17)))),
            poll!(rf.next())
        );
        assert_eq!(Poll::Ready(Some(Ok((vec!["baz"], 10)))), poll!(rf.next()));
        assert_eq!(Poll::Ready(None), poll!(rf.next()));
    }
}
