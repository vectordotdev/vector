use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Sink, Stream};
use pin_project::pin_project;
use tokio_stream::wrappers::ReceiverStream;

use crate::topology::{
    poll_sender::{PollSendError, PollSender},
    strategy::PollStrategy,
};
use crate::WhenFull;

/// A buffer sender.
///
/// The sender handles sending events into the buffer, as well as the behavior around handling
/// events when the internal channel is full.
///
/// When creating a buffer sender/receiver pair, callers can specify the "when full" behavior of the
/// sender.  This controls how events are handled when the internal channel is full.  Three modes
/// are possible:
/// - block
/// - drop newest
/// - overflow
///
/// In "block" mode, callers are simply forced to wait until the channel has enough capacity to
/// accept the event.  In "drop newest" mode, any event being sent when the channel is full will be
/// dropped and proceed no further. In "overflow" mode, events will be sent to another buffer
/// sender.  Callers can specify the overflow sender to use when constructing their buffers initially.
pub struct BufferSender<T> {
    base: PollSender<T>,
    base_ready: bool,
    drop_next: bool,
    overflow: Option<Pin<Box<BufferSender<T>>>>,
    overflow_ready: bool,
    when_full: WhenFull,
}

impl<T> BufferSender<T> {
    /// Creates a new [`BufferSender`] wrapping the given channel sender.
    pub(crate) fn new(sender: PollSender<T>, when_full: WhenFull) -> Self {
        Self {
            base: sender.into(),
            base_ready: false,
            drop_next: false,
            overflow: None,
            overflow_ready: false,
            when_full,
        }
    }

    /// Creates a new [`BufferSender`] wrapping the given channel sender and overflow sender.
    pub(crate) fn with_overflow(sender: PollSender<T>, overflow: BufferSender<T>) -> Self {
        Self {
            base: sender.into(),
            base_ready: false,
            drop_next: false,
            overflow: Some(Box::pin(overflow)),
            overflow_ready: false,
            when_full: WhenFull::Overflow,
        }
    }
}

impl<T: Send + 'static> BufferSender<T> {
    #[cfg(test)]
    pub(crate) fn get_base_ref(&self) -> &PollSender<T> {
        &self.base
    }

    #[cfg(test)]
    pub(crate) fn get_overflow_ref(&self) -> Option<&Pin<Box<BufferSender<T>>>> {
        self.overflow.as_ref()
    }

    // Pass through call to `PollSender<T>::start_send` to clean up the `Sink<T>` implementation.
    fn send_item(&mut self, item: T) -> Result<(), PollSendError<T>> {
        self.base.start_send(item)
    }

    // Pass through call to `PollSender<T>::close` to clean up the `Sink<T>` implementation.
    fn close(&mut self) {
        self.base.close();
    }
}

impl<T> Sink<T> for BufferSender<T>
where
    T: fmt::Debug + Send + 'static,
{
    type Error = PollSendError<T>;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Normally, calling `poll_ready` again after getting back a successful result from a
        // previous `poll_ready` call would be bad.  There's no way for a caller to know if the
        // implementation of `poll_ready` is actually allocating a resource or reserving a spot
        // every single time it's called, or if it will respond successfully based on a prior call
        // until that resource is consumed, etc.
        //
        // In this particular case, `PollSender<T>` will not reserve another sending slot if it has
        // already acquired one that has not been used yet.  Thus, it is safe for us to call
        // `poll_reserve` multiple times.  If either the base or overflow are ready, it means we got
        // back a successful response from `poll_reserve`, so we can just short circuit here.
        if self.base_ready || self.overflow_ready {
            return Poll::Ready(Ok(()));
        }

        let result = match self.base.poll_reserve(cx) {
            Poll::Ready(result) => match result {
                // We reserved a sending slot in the base channel.
                Ok(()) => {
                    self.base_ready = true;
                    Poll::Ready(Ok(()))
                }
                // Base sender's underlying channel is closed.
                Err(e) => Poll::Ready(Err(e)),
            },
            // Our base sender was not able to immediately reserve a sending slot.
            Poll::Pending => match self.when_full {
                // We need to block.  Nothing else to do, as the base sender will notify us when
                // there's capacity to do the send.
                WhenFull::Block => Poll::Pending,
                // We need to drop the next item.  We have to wait until the caller hands it over to
                // us in order to drop it, though, so we pretend we're ready and mark ourselves to
                // drop the next item when `start_send` is called.
                //
                // One "gotcha" here is that the base sender is still trying to reserve a sending
                // slot for us, so technically it could complete between now and when we get to
                // `start_send` and actually drop the item.
                //
                // Based on the current behavior of `PollSender<T>`, the best thing we can do here
                // is to simply to drop the item and not abort the send, since that will leave
                // `PollSender<T>` armed for the next time we call `poll_reserve`.  Since buffers
                // are SPSC, there's no risk in trying up a sender slot.
                //
                // TODO: In the future, `PollSender<T>::start_send` may be tweaked to attempt a
                // call to `Sender<T>::try_send` as a last ditch effort when `PollSender<T>` has not
                // yet reserved the sending slot.  We could take advantage of this ourselves.
                WhenFull::DropNewest => {
                    self.drop_next = true;
                    Poll::Ready(Ok(()))
                }
                // We're supposed to overflow.  Quickly check to make sure we even have an overflow
                // sender configured, and then figure out if the overflow sender can actually accept
                // a send at the moment.
                WhenFull::Overflow => match self.overflow.as_mut() {
                    None => panic!("overflow mode set, but no overflow sender present"),
                    Some(overflow) => match overflow.as_mut().poll_ready(cx) {
                        // Our overflow sender is ready for sending, so we mark ourselves so we know
                        // which sender to write to when `start_send` is called next.
                        Poll::Ready(result) => match result {
                            Ok(()) => {
                                self.overflow_ready = true;
                                Poll::Ready(Ok(()))
                            }
                            Err(e) => Poll::Ready(Err(e)),
                        },
                        // Our overflow sender is not ready, either, so there's nothing else to do
                        // here except wait for a wakeup from either the base sender or overflow sender.
                        Poll::Pending => Poll::Pending,
                    },
                },
            },
        };
        result
    }

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        // TODO: we should probably use a state enum for base ready vs overflow ready vs drop next

        // If we've been instructed to drop the next item, do so now.
        if self.drop_next {
            // We've been instructed to drop the next item.
            //
            // TODO: need to emit a metric here that we dropped
            drop(item);
            self.drop_next = false;
            return Ok(());
        }

        if !self.base_ready && !self.overflow_ready {
            // TODO: I don't super like panicking but this feels fine for the current design phase.
            panic!("`start_send` should not be called unless `poll_ready` returned successfully");
        }

        if self.base_ready {
            let result = self.send_item(item);
            self.base_ready = false;
            result
        } else {
            let result = match self.overflow.as_mut() {
                None => panic!("overflow mode set, but no overflow sender present"),
                Some(overflow) => overflow.as_mut().start_send(item),
            };
            self.overflow_ready = false;
            result
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Flushing is a no-op because `PollSender<T>` reserves a slot for sending up front when we
        // call `poll_reserve`, and if it gets the permit for sending, the
        // `PollSender<T>::start_send` call is guaranteed not to fail.
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        // Closing is always immediate: it's a state transition in `PollSender<T>`.  The only
        // "gotcha" is that an in-flight call to reserve a sending slot may still be pending, or it
        // may have completed and we're holding the permit.  However, none of that matters if we're
        // the ones closing, because we're eventually going to drop ourselves anyways, which will
        // then drop the underlying channel sender.
        //
        // There could, conceivably, I suppose, be an issue where we have `poll_close` called after
        // calling `poll_ready` successfully but never following up with a call to `start_send`...
        // and then before our top-level `BufferSender<T>` can be dropped, we wait for the
        // `BufferReceiver<T>` to drop, which it can't because we're still referencing the channel?
        //
        // It doesn't seem possible based on the fact we drive shutdown by dropping channels from
        // sources and letting the closures cascade from there.
        self.close();
        if let Some(overflow) = self.overflow.as_mut() {
            overflow.as_mut().get_mut().close();
        }

        Poll::Ready(Ok(()))
    }
}

/// A buffer receiver.
///
/// The receiver handles retrieving events from the buffer, regardless of the overall buffer configuration.
///
/// If a buffer was configured to operate in "overflow" mode, then the receiver will be responsible
/// for querying the overflow buffer as well.  The ordering of events when operating in "overflow"
/// is undefined, as the receiver will try to manage polling both its own buffer, as well as the
/// overflow buffer, in order to fairly balance throughput.
#[pin_project]
pub struct BufferReceiver<T> {
    #[pin]
    base: ReceiverStream<T>,
    overflow: Option<Box<BufferReceiver<T>>>,
    strategy: PollStrategy,
}

impl<T> BufferReceiver<T> {
    /// Creates a new [`BufferReceiver`] wrapping the given channel receiver.
    pub(crate) fn new(receiver: ReceiverStream<T>) -> Self {
        Self {
            base: receiver,
            overflow: None,
            strategy: PollStrategy::default(),
        }
    }

    /// Creates a new [`BufferReceiver`] wrapping the given channel receiver and overflow receiver.
    pub(crate) fn with_overflow(receiver: ReceiverStream<T>, overflow: BufferReceiver<T>) -> Self {
        Self {
            base: receiver,
            overflow: Some(Box::new(overflow)),
            strategy: PollStrategy::default(),
        }
    }
}

impl<T> Stream for BufferReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // We want to poll both our base and overflow receivers without waiting for one or the
        // other to entirely drain before checking the other.  This ensures that we're fairly
        // servicing both receivers, and avoiding stalls in one or the other.
        //
        // This is primarily important in situations where an overflow-triggering event has
        // occurred, and is over, and items are flowing through the base receiver.  If we waited to
        // entirely drain the overflow receiver, we might cause another small stall of the pipeline
        // attached to the base receiver.

        let this = self.project();
        let primary = this.base;
        let secondary = this.overflow.as_mut().map(Pin::new);

        this.strategy.poll_streams(primary, secondary, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fmt,
        sync::Arc,
        time::{Duration, Instant},
    };

    use futures::{SinkExt, StreamExt};
    use tokio::{
        pin,
        sync::{mpsc::channel, Barrier},
        time::sleep,
    };
    use tokio_stream::wrappers::ReceiverStream;

    use crate::{
        topology::{
            builder::IntoBuffer,
            channel::{BufferReceiver, BufferSender},
            poll_sender::PollSender,
        },
        WhenFull,
    };

    struct PassthroughChannel {
        capacity: usize,
    }

    impl PassthroughChannel {
        fn new(capacity: usize) -> Self {
            PassthroughChannel { capacity }
        }
    }

    impl IntoBuffer<u64> for PassthroughChannel {
        fn into_buffer_parts(self) -> (PollSender<u64>, ReceiverStream<u64>) {
            let (tx, rx) = channel(self.capacity);

            (PollSender::new(tx), ReceiverStream::new(rx))
        }
    }

    fn build_buffer(
        capacity: usize,
        mode: WhenFull,
        overflow_mode: Option<WhenFull>,
    ) -> (BufferSender<u64>, BufferReceiver<u64>) {
        match mode {
            WhenFull::Block | WhenFull::DropNewest => {
                let passthrough = PassthroughChannel::new(capacity);
                let (sender, receiver) = passthrough.into_buffer_parts();
                let sender = BufferSender::new(sender, mode);
                let receiver = BufferReceiver::new(receiver);
                (sender, receiver)
            }
            WhenFull::Overflow => {
                let overflow_mode = overflow_mode
                    .expect("overflow_mode must be specified when base is in overflow mode");
                let overflow_channel = PassthroughChannel::new(capacity);
                let (overflow_sender, overflow_receiver) = overflow_channel.into_buffer_parts();
                let overflow_sender = BufferSender::new(overflow_sender, overflow_mode);
                let overflow_receiver = BufferReceiver::new(overflow_receiver);

                let base_channel = PassthroughChannel::new(capacity);
                let (base_sender, base_receiver) = base_channel.into_buffer_parts();
                let base_sender = BufferSender::with_overflow(base_sender, overflow_sender);
                let base_receiver = BufferReceiver::with_overflow(base_receiver, overflow_receiver);

                (base_sender, base_receiver)
            }
        }
    }

    fn get_base_sender_capacity<T: Send + 'static>(sender: &BufferSender<T>) -> usize {
        sender
            .get_base_ref()
            .get_ref()
            .expect("channel should be live")
            .capacity()
    }

    fn get_overflow_sender_capacity<T: Send + 'static>(sender: &BufferSender<T>) -> Option<usize> {
        sender
            .get_overflow_ref()
            .and_then(|s| s.get_base_ref().get_ref())
            .map(|s| s.capacity())
    }

    fn assert_current_send_capacity<T>(
        sender: &mut BufferSender<T>,
        base_expected: usize,
        overflow_expected: Option<usize>,
    ) where
        T: fmt::Debug + Send + 'static,
    {
        assert_eq!(get_base_sender_capacity(&sender), base_expected);
        assert_eq!(get_overflow_sender_capacity(&sender), overflow_expected);
    }

    async fn assert_send_ok_with_capacities<T>(
        sender: &mut BufferSender<T>,
        value: T,
        base_expected: usize,
        overflow_expected: Option<usize>,
    ) where
        T: fmt::Debug + Send + 'static,
    {
        assert!(sender.send(value).await.is_ok());
        assert_current_send_capacity(sender, base_expected, overflow_expected);
    }

    async fn blocking_send_and_drain_receiver<T>(
        mut sender: BufferSender<T>,
        receiver: BufferReceiver<T>,
        send_value: T,
    ) -> Vec<T>
    where
        T: fmt::Debug + Send + 'static,
    {
        // We can likely replace this with `tokio_test`-related helpers to avoid the sleeping.
        let send_baton = Arc::new(Barrier::new(2));
        let recv_baton = Arc::clone(&send_baton);
        let recv_delay = Duration::from_millis(500);
        let handle = tokio::spawn(async move {
            let mut results = Vec::new();
            pin!(receiver);

            // Synchronize with sender and then wait for a small period of time to simulate a
            // blocking delay.
            let _ = recv_baton.wait().await;
            sleep(recv_delay).await;

            // Grab all messages and then return the results.
            while let Some(msg) = receiver.next().await {
                results.push(msg);
            }
            results
        });

        // We also have to drop our sender after sending the fourth message so that the receiver
        // task correctly exits.  If we didn't drop it, the receiver task would just assume that we
        // had no more messages to send, waiting for-ev-er for the next one.
        let start = Instant::now();
        let _ = send_baton.wait().await;
        assert!(sender.send(send_value).await.is_ok());
        let send_delay = start.elapsed();
        assert!(send_delay > recv_delay);
        drop(sender);

        handle.await.expect("receiver task should not panic")
    }

    async fn drain_receiver<T>(sender: BufferSender<T>, receiver: BufferReceiver<T>) -> Vec<T>
    where
        T: fmt::Debug + Send + 'static,
    {
        drop(sender);
        let handle = tokio::spawn(async move {
            let mut results = Vec::new();
            pin!(receiver);

            // Grab all messages and then return the results.
            while let Some(msg) = receiver.next().await {
                results.push(msg);
            }
            results
        });

        handle.await.expect("receiver task should not panic")
    }

    #[tokio::test]
    async fn test_sender_block() {
        // Get a non-overflow buffer in blocking mode with a capacity of 3.
        let (mut tx, rx) = build_buffer(3, WhenFull::Block, None);

        // We should be able to send three messages through unimpeded.
        assert_current_send_capacity(&mut tx, 3, None);
        assert_send_ok_with_capacities(&mut tx, 1, 2, None).await;
        assert_send_ok_with_capacities(&mut tx, 2, 1, None).await;
        assert_send_ok_with_capacities(&mut tx, 3, 0, None).await;

        // Our next send _should_ block.  `assert_sender_blocking_send_and_recv` spawns a receiver
        // task which waits for a small period of time, and we track how long our next send blocks
        // for, which should be greater than the time that the receiver task waits.  This asserts
        // that the send is blocking, and that it's dependent on the receiver.
        //
        // It also drops the sender and receives all remaining messages on the receiver, returning
        // them to us to check.
        let mut results = blocking_send_and_drain_receiver(tx, rx, 4).await;
        results.sort();
        assert_eq!(results, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_sender_drop_newest() {
        // Get a non-overflow buffer in "drop newest" mode with a capacity of 3.
        let (mut tx, rx) = build_buffer(3, WhenFull::DropNewest, None);

        // We should be able to send three messages through unimpeded.
        assert_current_send_capacity(&mut tx, 3, None);
        assert_send_ok_with_capacities(&mut tx, 1, 2, None).await;
        assert_send_ok_with_capacities(&mut tx, 2, 1, None).await;
        assert_send_ok_with_capacities(&mut tx, 3, 0, None).await;

        // Then, since we're in "drop newest" mode, we could continue to send without issue or being
        // blocked, but we would except those items to, well.... be dropped.
        assert_send_ok_with_capacities(&mut tx, 7, 0, None).await;
        assert_send_ok_with_capacities(&mut tx, 8, 0, None).await;
        assert_send_ok_with_capacities(&mut tx, 9, 0, None).await;

        // Then, when we collect all of the messages from the receiver, we should only get back the
        // first three of them.
        let mut results = drain_receiver(tx, rx).await;
        results.sort();
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_sender_overflow_block() {
        // Get an overflow buffer, where the overflow buffer is in blocking mode, and both the base
        // and overflow buffers have a capacity of 2.
        let (mut tx, rx) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::Block));

        // We should be able to send four message through unimpeded -- two for the base sender, and
        // two for the overflow sender.
        assert_current_send_capacity(&mut tx, 2, Some(2));
        assert_send_ok_with_capacities(&mut tx, 1, 1, Some(2)).await;
        assert_send_ok_with_capacities(&mut tx, 2, 0, Some(2)).await;
        assert_send_ok_with_capacities(&mut tx, 3, 0, Some(1)).await;
        assert_send_ok_with_capacities(&mut tx, 4, 0, Some(0)).await;

        // Our next send _should_ block.  `assert_sender_blocking_send_and_recv` spawns a receiver
        // task which waits for a small period of time, and we track how long our next send blocks
        // for, which should be greater than the time that the receiver task waits.  This asserts
        // that the send is blocking, and that it's dependent on the receiver.
        //
        // It also drops the sender and receives all remaining messages on the receiver, returning
        // them to us to check.
        let mut results = blocking_send_and_drain_receiver(tx, rx, 5).await;
        results.sort();
        assert_eq!(results, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_sender_overflow_drop_newest() {
        // Get an overflow buffer, where the overflow buffer is in "drop newest" mode, and both the
        // base and overflow buffers have a capacity of 2.
        let (mut tx, rx) = build_buffer(2, WhenFull::Overflow, Some(WhenFull::DropNewest));

        // We should be able to send four message through unimpeded -- two for the base sender, and
        // two for the overflow sender.
        assert_current_send_capacity(&mut tx, 2, Some(2));
        assert_send_ok_with_capacities(&mut tx, 7, 1, Some(2)).await;
        assert_send_ok_with_capacities(&mut tx, 8, 0, Some(2)).await;
        assert_send_ok_with_capacities(&mut tx, 2, 0, Some(1)).await;
        assert_send_ok_with_capacities(&mut tx, 1, 0, Some(0)).await;

        // Then, since we're in "drop newest" mode on the overflow side, we could continue to send
        // without issue or being blocked, but we would except those items to, well.... be dropped.
        assert_send_ok_with_capacities(&mut tx, 5, 0, Some(0)).await;
        assert_send_ok_with_capacities(&mut tx, 6, 0, Some(0)).await;
        assert_send_ok_with_capacities(&mut tx, 3, 0, Some(0)).await;

        // Then, when we collect all of the messages from the receiver, we should only get back the
        // first four of them.
        let mut results = drain_receiver(tx, rx).await;
        results.sort();
        assert_eq!(results, vec![1, 2, 7, 8]);
    }
}
