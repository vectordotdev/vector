use crossbeam_queue::ArrayQueue;
use futures::{ready, task::AtomicWaker};
use std::{
    cmp, fmt,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use crate::Bufferable;

use super::{poll_notify::PollNotify, poll_semaphore::PollSemaphore};

/// Error returned by `LimitedSender`.
#[derive(Debug)]
pub struct SendError<T>(pub T);

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl<T: fmt::Debug> std::error::Error for SendError<T> {}

struct Inner<T> {
    data: Arc<ArrayQueue<(OwnedSemaphorePermit, T)>>,
    limit: usize,
    limiter: PollSemaphore,
    read_waker: Arc<AtomicWaker>,
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

pub struct LimitedSender<T> {
    inner: Inner<T>,
    slot: Option<T>,
}

pub struct LimitedReceiver<T> {
    inner: Inner<T>,
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
    pub fn capacity(&self) -> usize {
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
                if self.capacity() == 0 {
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
                        self.inner.read_waker.wake();

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
        let result = ready!(self.poll_flush(cx));

        self.inner.limiter.close();
        Poll::Ready(result)
    }
}

impl<T: Bufferable> LimitedReceiver<T> {
    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.inner.data.pop() {
            Some((permit, item)) => {
                // We got an item, woohoo! Now, drop the permit which will properly free up permits
                // in the semaphore, and then also try to notify a pending writer.
                drop(permit);
                self.inner.write_waker.as_ref().notify_one();
                Poll::Ready(Some(item))
            }
            // Figure out if we're actually closed or not, to determine if more items might be
            // coming or if it's time to also close up shop.
            None => {
                if self.inner.limiter.is_closed() {
                    Poll::Ready(None)
                } else {
                    self.inner.read_waker.register(cx.waker());
                    Poll::Pending
                }
            }
        }
    }
}

impl<T> Clone for LimitedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            slot: None,
        }
    }
}

impl<T> Drop for LimitedSender<T> {
    fn drop(&mut self) {
        self.inner.limiter.close();
    }
}

pub fn limited<T>(limit: usize) -> (LimitedSender<T>, LimitedReceiver<T>) {
    let inner = Inner {
        data: Arc::new(ArrayQueue::new(limit)),
        limit,
        limiter: PollSemaphore::new(Arc::new(Semaphore::new(limit))),
        read_waker: Arc::new(AtomicWaker::new()),
        write_waker: PollNotify::new(Arc::new(Notify::new())),
    };

    let sender = LimitedSender {
        inner: inner.clone(),
        slot: None,
    };
    let receiver = LimitedReceiver { inner };

    (sender, receiver)
}
