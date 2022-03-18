use crossbeam_queue::ArrayQueue;
use futures::{ready, Sink, Stream};
use std::{
    cmp, fmt,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use crate::Bufferable;

use super::{poll_notify::PollNotify, poll_semaphore::PollSemaphore};

/// Error returned by `LimitedSender`.
#[derive(Debug, PartialEq)]
pub struct SendError<T>(pub T);

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl<T: fmt::Debug> std::error::Error for SendError<T> {}

#[derive(Debug)]
struct Inner<T> {
    data: Arc<ArrayQueue<(OwnedSemaphorePermit, T)>>,
    limit: usize,
    limiter: PollSemaphore,
    read_waker: PollNotify,
    write_waker: PollNotify,
}

impl<T> Clone for Inner<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            limit: self.limit,
            limiter: self.limiter.clone(),
            read_waker: self.read_waker.clone(),
            write_waker: self.write_waker.clone(),
        }
    }
}

#[derive(Debug)]
pub struct LimitedSender<T> {
    inner: Inner<T>,
    sender_count: Arc<AtomicUsize>,
    slot: Option<T>,
}

impl<T: Bufferable> LimitedSender<T> {
    #[allow(clippy::cast_possible_truncation)]
    fn get_required_permits_for_item(&self, item: &T) -> u32 {
        // We have to limit the number of permits we ask for to the overall limit since we're always
        // willing to store more items than the limit if the queue is entirely empty, because
        // otherwise we might deadlock ourselves by not being able to send a single item.
        cmp::min(self.inner.limit, item.event_count()) as u32
    }

    /// Gets the number of items that this channel could accept.
    pub fn available_capacity(&self) -> usize {
        self.inner.limiter.available_permits()
    }

    /// Attempts to prepare the sender to receive a value.
    ///
    /// This method must be called and return `Poll::Ready(Ok(()))` prior to each call to
    /// `start_send`.
    ///
    /// This method returns `Poll::Ready` once the underlying sender is ready to receive data. If
    /// this method returns `Poll::Pending`, the current task is registered to be notified (via
    /// `cx.waker().wake_by_ref()`) when `poll_ready` should be called again.
    ///
    /// If this function encounters an error, the sender should be considered to have failed
    /// permanently, and should no longer be called.
    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        loop {
            if self.slot.is_none() {
                if self.available_capacity() == 0 {
                    // We have no capacity via the semaphore, so we need to wait for the reader to make
                    // some progress.  We set ourselves for a notification, but there might be a stored
                    // one hence our loop here, which should either drive us back around to a semaphore
                    // _with_ capacity or to a notifier that has no stored notification and thus truly
                    // registering ourselves for a later wake-up.
                    ready!(self.inner.write_waker.poll_notify(cx));
                } else {
                    // The semaphore now has some capacity, so we'll let the caller enqueue an item
                    // for sending, where we'll then continue driving the item towards actually
                    // sending, by truly acquiring enough permits for the given item size.
                    return Poll::Ready(Ok(()));
                }
            } else {
                // We have an item in the holding slot that we need to try and drive towards being
                // sent, so try and actually flush.
                return self.poll_flush(cx);
            }
        }
    }

    /// Begin the process of sending a value to the sender. Each call to this function must be
    /// preceded by a successful call to `poll_ready` which returned `Poll::Ready(Ok(()))`.
    ///
    /// As the name suggests, this method only begins the process of sending the item. The item
    /// isn’t fully processed until the buffer is fully flushed. You must use `poll_flush` or
    /// `poll_close` in order to guarantee completion of a send.
    ///
    /// If this function encounters an error, the sender should be considered to have failed
    /// permanently, and should no longer be called.
    ///
    /// # Errors
    ///
    /// If any item was already given via `start_send`, but has not yet been been fully sent by
    /// calling `poll_flush` until it returned `Poll::Ready(Ok(()))`, then an error variant will be
    /// returned containing the item that was already queued but not yet sent.
    pub fn start_send(&mut self, item: T) -> Result<(), SendError<T>> {
        // Attempt to store the item in our holding slot.  If there was already an item in the
        // holding slot, the caller badly violated the `Sink` contract so we need to panic and
        // loudly surface this.
        match self.slot.replace(item) {
            None => Ok(()),
            Some(old_item) => Err(SendError(old_item)),
        }
    }

    /// Flush any remaining output from this sender.
    ///
    /// Returns `Poll::Ready(Ok(()))` when no buffered items remain. If this value is returned then it
    /// is guaranteed that all previous values sent via `start_send` have been flushed.
    ///
    /// Returns `Poll::Pending` if there is more work left to do, in which case the current task is
    /// scheduled (via `cx.waker().wake_by_ref()`) to wake up when `poll_flush` should be called again.
    ///
    /// If this function encounters an error, the sender should be considered to have failed
    /// permanently, and should no longer be called.
    pub fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        match self.slot.take() {
            None => Poll::Ready(Ok(())),
            Some(item) => {
                let permits_n = self.get_required_permits_for_item(&item);
                match self.inner.limiter.poll_acquire_many(permits_n, cx) {
                    Poll::Ready(Some(permit)) => {
                        // We acquired enough permits to allow this send to proceed, so we bundle the
                        // permit with the item so that when we pull it out of the queue, the capacity
                        // is correctly adjusted.
                        self.inner.data.push((permit, item)).expect(
                            "queue should always have capacity when permits can be acquired",
                        );

                        // Don't forget to wake the reader since there's data to consume now. :)
                        self.inner.read_waker.as_ref().notify_one();

                        Poll::Ready(Ok(()))
                    }
                    // The semaphore is closed, so the sender is closed, so the caller should not still
                    // be calling us, so this is an error.
                    Poll::Ready(None) => Poll::Ready(Err(SendError(item))),
                    Poll::Pending => {
                        // We couldn't get all the permits yet, so we have to store the item back in the
                        // holding slot before returning.
                        self.slot = Some(item);
                        Poll::Pending
                    }
                }
            }
        }
    }

    /// Flush any remaining output and close this sender, if necessary.
    ///
    /// Returns `Poll::Ready(Ok(()))` when no buffered items remain and the sender has been successfully
    /// closed.
    ///
    /// Returns `Poll::Pending` if there is more work left to do, in which case the current task is
    /// scheduled (via `cx.waker().wake_by_ref()`) to wake up when `poll_close` should be called again.
    ///
    /// If this function encounters an error, the sender should be considered to have failed
    /// permanently, and should no longer be called.
    pub fn poll_close(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        // We can't close until any pending item is fully sent through.
        self.poll_flush(cx)
    }
}

impl<T: Bufferable> Sink<T> for LimitedSender<T> {
    type Error = SendError<T>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        Pin::into_inner(self).poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), SendError<T>> {
        Pin::into_inner(self).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        Pin::into_inner(self).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        Pin::into_inner(self).poll_close(cx)
    }
}

#[derive(Debug)]
pub struct LimitedReceiver<T> {
    inner: Inner<T>,
}

impl<T: Bufferable> LimitedReceiver<T> {
    /// Gets the number of items that this channel could accept.
    pub fn available_capacity(&self) -> usize {
        self.inner.limiter.available_permits()
    }

    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        loop {
            if let Some((permit, item)) = self.inner.data.pop() {
                // We got an item, woohoo! Now, drop the permit which will properly free up permits
                // in the semaphore, and then also try to notify a pending writer.
                drop(permit);
                self.inner.write_waker.as_ref().notify_one();

                return Poll::Ready(Some(item));
            }

            // There wasn't an item for us to pop, so see if the channel is actually closed.  If so,
            // then it's time for us to close up shop as well.
            if self.inner.limiter.is_closed() {
                return Poll::Ready(None);
            }

            // We're not closed, so we need to wait for a writer to tell us they made some
            // progress.  This might end up being a spurious wakeup since `Notify` will
            // store up to one wakeup that gets consumed by the next call to `poll_notify`,
            // but alas.
            ready!(self.inner.read_waker.poll_notify(cx));
        }
    }

    pub fn close(&mut self) {
        self.inner.limiter.close();
    }
}

impl<T: Bufferable> Stream for LimitedReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        Pin::into_inner(self).poll_next(cx)
    }
}

impl<T> Clone for LimitedSender<T> {
    fn clone(&self) -> Self {
        self.sender_count.fetch_add(1, Ordering::SeqCst);

        Self {
            inner: self.inner.clone(),
            sender_count: Arc::clone(&self.sender_count),
            slot: None,
        }
    }
}

impl<T> Drop for LimitedSender<T> {
    fn drop(&mut self) {
        // If we're the last sender to drop, close the semaphore on our way out the door.
        if self.sender_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.inner.limiter.close();
            self.inner.read_waker.as_ref().notify_one();
        }
    }
}

pub fn limited<T>(limit: usize) -> (LimitedSender<T>, LimitedReceiver<T>) {
    let inner = Inner {
        data: Arc::new(ArrayQueue::new(limit)),
        limit,
        limiter: PollSemaphore::new(Arc::new(Semaphore::new(limit))),
        read_waker: PollNotify::new(Arc::new(Notify::new())),
        write_waker: PollNotify::new(Arc::new(Notify::new())),
    };

    let sender = LimitedSender {
        inner: inner.clone(),
        sender_count: Arc::new(AtomicUsize::new(1)),
        slot: None,
    };
    let receiver = LimitedReceiver { inner };

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use futures::future::poll_fn;
    use tokio_test::{assert_pending, assert_ready, task::spawn};

    use crate::{test::common::MultiEventRecord, topology::channel::limited_queue::SendError};

    use super::limited;

    #[test]
    fn send_receive() {
        let (mut tx, mut rx) = limited(2);

        assert_eq!(2, tx.available_capacity());

        // Create our send and receive futures.
        let mut send = spawn(async {
            let msg: u64 = 42;

            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(msg)?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        let mut recv = spawn(poll_fn(|cx| rx.poll_next(cx)));

        // Nobody should be woken up.
        assert!(!send.is_woken());
        assert!(!recv.is_woken());

        // Try polling our receive, which should be pending because we haven't anything yet.
        assert_pending!(recv.poll());

        // We should immediately be able to complete a send as there is available capacity.
        assert_eq!(Ok(()), assert_ready!(send.poll()));

        // Now our receive should have been woken up, and should immediately be ready.
        assert!(recv.is_woken());
        assert_eq!(Some(42), assert_ready!(recv.poll()));
    }

    #[test]
    fn sender_waits_for_more_capacity_when_none_available() {
        let (mut tx, mut rx) = limited(1);

        assert_eq!(1, tx.available_capacity());

        // Create our send and receive futures.
        let mut send1 = spawn(async {
            let msg: u64 = 42;

            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(msg)?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        let mut recv1 = spawn(poll_fn(|cx| rx.poll_next(cx)));

        // Nobody should be woken up.
        assert!(!send1.is_woken());
        assert!(!recv1.is_woken());

        // Try polling our receive, which should be pending because we haven't anything yet.
        assert_pending!(recv1.poll());

        // We should immediately be able to complete a send as there is available capacity.
        assert_eq!(Ok(()), assert_ready!(send1.poll()));
        drop(send1);

        assert_eq!(0, tx.available_capacity());

        // Now our receive should have been woken up, and should immediately be ready... but we
        // aren't going to read the value just yet.
        assert!(recv1.is_woken());

        // Now trigger a second send, which should block as there's no available capacity.
        let mut send2 = spawn(async {
            let msg: u64 = 43;

            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(msg)?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        assert!(!send2.is_woken());
        assert_pending!(send2.poll());

        // Now if we receive the item, our second send should be woken up and be able to send in.
        assert_eq!(Some(42), assert_ready!(recv1.poll()));
        drop(recv1);

        assert_eq!(1, rx.available_capacity());

        let mut recv2 = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert!(!recv2.is_woken());
        assert_pending!(recv2.poll());

        assert!(send2.is_woken());
        assert_eq!(Ok(()), assert_ready!(send2.poll()));
        drop(send2);

        assert_eq!(0, tx.available_capacity());

        // And the final receive to get our second send.
        assert!(recv2.is_woken());
        assert_eq!(Some(43), assert_ready!(recv2.poll()));

        assert_eq!(1, tx.available_capacity());
    }

    #[test]
    fn sender_waits_for_more_capacity_when_partial_available() {
        let (mut tx, mut rx) = limited(7);

        assert_eq!(7, tx.available_capacity());

        // Create our send and receive futures.
        let mut small_sends = spawn(async {
            let msgs = vec![
                MultiEventRecord(1),
                MultiEventRecord(2),
                MultiEventRecord(3),
            ];

            for msg in msgs {
                poll_fn(|cx| tx.poll_ready(cx)).await?;
                tx.start_send(msg)?;
                poll_fn(|cx| tx.poll_flush(cx)).await?;
            }

            Ok::<_, SendError<MultiEventRecord>>(())
        });

        let mut recv1 = spawn(poll_fn(|cx| rx.poll_next(cx)));

        // Nobody should be woken up.
        assert!(!small_sends.is_woken());
        assert!(!recv1.is_woken());

        // Try polling our receive, which should be pending because we haven't anything yet.
        assert_pending!(recv1.poll());

        // We should immediately be able to complete our three event sends, which we have
        // available capacity for, but will consume all but one of the available slots.
        assert_eq!(Ok(()), assert_ready!(small_sends.poll()));
        drop(small_sends);

        assert_eq!(1, tx.available_capacity());

        // Now our receive should have been woken up, and should immediately be ready, but we won't
        // receive just yet.
        assert!(recv1.is_woken());

        // Now trigger a second send that has four events, and needs to wait for two receives to happen.
        let mut send2 = spawn(async {
            let msg = MultiEventRecord(4);

            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(msg)?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        assert!(!send2.is_woken());
        assert_pending!(send2.poll());

        // Now if we receive the first item, our second send should be woken up but still not able
        // to send.
        assert_eq!(Some(MultiEventRecord(1)), assert_ready!(recv1.poll()));
        drop(recv1);

        // Callers waiting to acquire permits have the permits immediately transfer to them when one
        // (or more) are released, so we expect this to be zero until we send and then read the
        // third item.
        assert_eq!(0, rx.available_capacity());

        // We don't get woken up until all permits have been acquired.
        assert!(!send2.is_woken());

        // Our second read should unlock enough available capacity for the second send once complete.
        let mut recv2 = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert!(!recv2.is_woken());
        assert_eq!(Some(MultiEventRecord(2)), assert_ready!(recv2.poll()));
        drop(recv2);

        assert_eq!(0, rx.available_capacity());

        assert!(send2.is_woken());
        assert_eq!(Ok(()), assert_ready!(send2.poll()));

        // And just make sure we see those last two sends.
        let mut recv3 = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert!(!recv3.is_woken());
        assert_eq!(Some(MultiEventRecord(3)), assert_ready!(recv3.poll()));
        drop(recv3);

        assert_eq!(3, rx.available_capacity());

        let mut recv4 = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert!(!recv4.is_woken());
        assert_eq!(Some(MultiEventRecord(4)), assert_ready!(recv4.poll()));
        drop(recv4);

        assert_eq!(7, rx.available_capacity());
    }

    #[test]
    fn empty_receiver_returns_none_when_last_sender_drops() {
        let (mut tx, mut rx) = limited(1);

        assert_eq!(1, tx.available_capacity());

        let tx2 = tx.clone();

        // Create our send and receive futures.
        let mut send = spawn(async {
            let msg: u64 = 42;

            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(msg)?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        let mut recv = spawn(poll_fn(|cx| rx.poll_next(cx)));

        // Nobody should be woken up.
        assert!(!send.is_woken());
        assert!(!recv.is_woken());

        // Try polling our receive, which should be pending because we haven't anything yet.
        assert_pending!(recv.poll());

        // Now drop our second sender, which shouldn't do anything yet.
        drop(tx2);
        assert!(!recv.is_woken());
        assert_pending!(recv.poll());

        // Now drop our second sender, but not before doing a send, which should trigger closing the
        // semaphore which should let the receiver complete with no further waiting: one item and
        // then `None`.
        assert_eq!(Ok(()), assert_ready!(send.poll()));
        drop(send);
        drop(tx);

        assert!(recv.is_woken());
        assert_eq!(Some(42), assert_ready!(recv.poll()));
        drop(recv);

        let mut recv2 = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert!(!recv2.is_woken());
        assert_eq!(None, assert_ready!(recv2.poll()));
    }

    #[test]
    fn receiver_returns_none_once_empty_when_last_sender_drops() {
        let (tx, mut rx) = limited::<u64>(1);

        assert_eq!(1, tx.available_capacity());

        let tx2 = tx.clone();

        // Create our receive future.
        let mut recv = spawn(poll_fn(|cx| rx.poll_next(cx)));

        // Nobody should be woken up.
        assert!(!recv.is_woken());

        // Try polling our receive, which should be pending because we haven't anything yet.
        assert_pending!(recv.poll());

        // Now drop our first sender, which shouldn't do anything yet.
        drop(tx);
        assert!(!recv.is_woken());
        assert_pending!(recv.poll());

        // Now drop our second sender, which should trigger closing the semaphore which should let
        // the receive complete as there are no items to read.
        drop(tx2);
        assert!(recv.is_woken());
        assert_eq!(None, assert_ready!(recv.poll()));
    }

    #[test]
    fn oversized_send_allowed_when_empty() {
        let (mut tx, mut rx) = limited(1);

        assert_eq!(1, tx.available_capacity());

        // Create our send and receive futures.
        let mut send = spawn(async {
            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(MultiEventRecord(2))?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        let mut recv = spawn(poll_fn(|cx| rx.poll_next(cx)));

        // Nobody should be woken up.
        assert!(!send.is_woken());
        assert!(!recv.is_woken());

        // We should immediately be able to complete our send, which we don't have full
        // available capacity for, but will consume all of the available slots.
        assert_eq!(Ok(()), assert_ready!(send.poll()));
        drop(send);

        assert_eq!(0, tx.available_capacity());

        // Now we should be able to get back the oversized item, but our capacity should not be
        // greater than what we started with.
        assert_eq!(Some(MultiEventRecord(2)), assert_ready!(recv.poll()));
        drop(recv);

        assert_eq!(1, rx.available_capacity());
    }

    #[test]
    fn oversized_send_allowed_when_partial_capacity() {
        let (mut tx, mut rx) = limited(2);

        assert_eq!(2, tx.available_capacity());

        // Create our send future.
        let mut send = spawn(async {
            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(MultiEventRecord(1))?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        // Nobody should be woken up.
        assert!(!send.is_woken());

        // We should immediately be able to complete our send, which will only use up a single slot.
        assert_eq!(Ok(()), assert_ready!(send.poll()));
        drop(send);

        assert_eq!(1, tx.available_capacity());

        // Now we'll trigger another send which has an oversized item.  It shouldn't be able to send
        // until all permits are available.
        let mut send2 = spawn(async {
            poll_fn(|cx| tx.poll_ready(cx)).await?;
            tx.start_send(MultiEventRecord(3))?;
            poll_fn(|cx| tx.poll_flush(cx)).await
        });

        assert!(!send2.is_woken());
        assert_pending!(send2.poll());

        assert_eq!(0, rx.available_capacity());

        // Now do a receive which should return the one consumed slot, essentially allowing all
        // permits to be acquired by the blocked send.
        let mut recv = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert!(!recv.is_woken());
        assert!(!send2.is_woken());

        assert_eq!(Some(MultiEventRecord(1)), assert_ready!(recv.poll()));
        drop(recv);

        assert_eq!(0, rx.available_capacity());

        // Now our blocked send should be able to proceed, and we should be able to read back the
        // item.
        assert_eq!(Ok(()), assert_ready!(send2.poll()));
        drop(send2);

        assert_eq!(0, tx.available_capacity());

        let mut recv2 = spawn(poll_fn(|cx| rx.poll_next(cx)));
        assert_eq!(Some(MultiEventRecord(3)), assert_ready!(recv2.poll()));

        assert_eq!(2, tx.available_capacity());
    }
}
