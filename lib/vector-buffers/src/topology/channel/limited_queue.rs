use std::{
    cmp, fmt,
    fmt::Debug,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[cfg(test)]
use std::sync::Mutex;

use async_stream::stream;
use crossbeam_queue::{ArrayQueue, SegQueue};
use futures::Stream;
use metrics::{Gauge, Histogram, gauge, histogram};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, TryAcquireError};

use crate::{InMemoryBufferable, config::MemoryBufferSize};

/// Error returned by `LimitedSender::send` when the receiver has disconnected.
#[derive(Debug, PartialEq, Eq)]
pub struct SendError<T>(pub T);

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "receiver disconnected")
    }
}

impl<T: fmt::Debug> std::error::Error for SendError<T> {}

/// Error returned by `LimitedSender::try_send`.
#[derive(Debug, PartialEq, Eq)]
pub enum TrySendError<T> {
    InsufficientCapacity(T),
    Disconnected(T),
}

impl<T> TrySendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            Self::InsufficientCapacity(item) | Self::Disconnected(item) => item,
        }
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientCapacity(_) => {
                write!(fmt, "channel lacks sufficient capacity for send")
            }
            Self::Disconnected(_) => write!(fmt, "receiver disconnected"),
        }
    }
}

impl<T: fmt::Debug> std::error::Error for TrySendError<T> {}

// Trait over common queue operations so implementation can be chosen at initialization phase
trait QueueImpl<T>: Send + Sync + fmt::Debug {
    fn push(&self, item: T);
    fn pop(&self) -> Option<T>;
}

impl<T> QueueImpl<T> for ArrayQueue<T>
where
    T: Send + Sync + fmt::Debug,
{
    fn push(&self, item: T) {
        self.push(item)
            .unwrap_or_else(|_| unreachable!("acquired permits but channel reported being full."));
    }

    fn pop(&self) -> Option<T> {
        self.pop()
    }
}

impl<T> QueueImpl<T> for SegQueue<T>
where
    T: Send + Sync + fmt::Debug,
{
    fn push(&self, item: T) {
        self.push(item);
    }

    fn pop(&self) -> Option<T> {
        self.pop()
    }
}

#[derive(Clone, Debug)]
pub struct ChannelMetricMetadata {
    prefix: &'static str,
    output: Option<String>,
}

impl ChannelMetricMetadata {
    pub fn new(prefix: &'static str, output: Option<String>) -> Self {
        Self { prefix, output }
    }
}

#[derive(Clone, Debug)]
struct Metrics {
    histogram: Histogram,
    gauge: Gauge,
    // We hold a handle to the max gauge to avoid it being dropped by the metrics collector, but
    // since the value is static, we never need to update it. The compiler detects this as an unused
    // field, so we need to suppress the warning here.
    #[expect(dead_code)]
    max_gauge: Gauge,
    #[cfg(test)]
    recorded_values: Arc<Mutex<Vec<usize>>>,
}

impl Metrics {
    #[expect(clippy::cast_precision_loss)] // We have to convert buffer sizes for a gauge, it's okay to lose precision here.
    fn new(limit: MemoryBufferSize, metadata: ChannelMetricMetadata) -> Self {
        let ChannelMetricMetadata { prefix, output } = metadata;
        let (gauge_suffix, max_value) = match limit {
            MemoryBufferSize::MaxEvents(max_events) => ("_max_event_size", max_events.get() as f64),
            MemoryBufferSize::MaxSize(max_bytes) => ("_max_byte_size", max_bytes.get() as f64),
        };
        let max_gauge_name = format!("{prefix}{gauge_suffix}");
        let histogram_name = format!("{prefix}_utilization");
        let gauge_name = format!("{prefix}_utilization_level");
        #[cfg(test)]
        let recorded_values = Arc::new(Mutex::new(Vec::new()));
        if let Some(label_value) = output {
            let max_gauge = gauge!(max_gauge_name, "output" => label_value.clone());
            max_gauge.set(max_value);
            Self {
                histogram: histogram!(histogram_name, "output" => label_value.clone()),
                gauge: gauge!(gauge_name, "output" => label_value.clone()),
                max_gauge,
                #[cfg(test)]
                recorded_values,
            }
        } else {
            let max_gauge = gauge!(max_gauge_name);
            max_gauge.set(max_value);
            Self {
                histogram: histogram!(histogram_name),
                gauge: gauge!(gauge_name),
                max_gauge,
                #[cfg(test)]
                recorded_values,
            }
        }
    }

    #[expect(clippy::cast_precision_loss)]
    fn record(&self, value: usize) {
        self.histogram.record(value as f64);
        self.gauge.set(value as f64);
        #[cfg(test)]
        if let Ok(mut recorded) = self.recorded_values.lock() {
            recorded.push(value);
        }
    }
}

#[derive(Debug)]
struct Inner<T> {
    data: Arc<dyn QueueImpl<(OwnedSemaphorePermit, T)>>,
    limit: MemoryBufferSize,
    limiter: Arc<Semaphore>,
    read_waker: Arc<Notify>,
    metrics: Option<Metrics>,
}

impl<T> Clone for Inner<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            limit: self.limit,
            limiter: self.limiter.clone(),
            read_waker: self.read_waker.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl<T: InMemoryBufferable> Inner<T> {
    fn new(limit: MemoryBufferSize, metric_metadata: Option<ChannelMetricMetadata>) -> Self {
        let read_waker = Arc::new(Notify::new());
        let metrics = metric_metadata.map(|metadata| Metrics::new(limit, metadata));
        match limit {
            MemoryBufferSize::MaxEvents(max_events) => Inner {
                data: Arc::new(ArrayQueue::new(max_events.get())),
                limit,
                limiter: Arc::new(Semaphore::new(max_events.get())),
                read_waker,
                metrics,
            },
            MemoryBufferSize::MaxSize(max_bytes) => Inner {
                data: Arc::new(SegQueue::new()),
                limit,
                limiter: Arc::new(Semaphore::new(max_bytes.get())),
                read_waker,
                metrics,
            },
        }
    }

    /// Records a send after acquiring all required permits.
    ///
    /// The `total` value represents the channel utilization after this send completes.  It may be
    /// greater than the configured limit because the channel intentionally allows a single
    /// oversized payload to flow through rather than forcing the sender to split it.
    fn send_with_permit(&mut self, total: usize, permits: OwnedSemaphorePermit, item: T) {
        self.data.push((permits, item));
        self.read_waker.notify_one();
        // Due to the race between getting the available capacity, acquiring the permits, and the
        // above push, the total may be inaccurate. Record it anyways as the histogram totals will
        // _eventually_ converge on a true picture of the buffer utilization.
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.record(total);
        }
    }
}

#[derive(Debug)]
pub struct LimitedSender<T> {
    inner: Inner<T>,
    sender_count: Arc<AtomicUsize>,
}

impl<T: InMemoryBufferable> LimitedSender<T> {
    #[allow(clippy::cast_possible_truncation)]
    fn calc_required_permits(&self, item: &T) -> (usize, usize, u32) {
        // We have to limit the number of permits we ask for to the overall limit since we're always
        // willing to store more items than the limit if the queue is entirely empty, because
        // otherwise we might deadlock ourselves by not being able to send a single item.
        let (limit, value) = match self.inner.limit {
            MemoryBufferSize::MaxSize(max_size) => (max_size, item.allocated_bytes()),
            MemoryBufferSize::MaxEvents(max_events) => (max_events, item.event_count()),
        };
        let limit = limit.get();
        (limit, value, cmp::min(limit, value) as u32)
    }

    /// Gets the number of items that this channel could accept.
    pub fn available_capacity(&self) -> usize {
        self.inner.limiter.available_permits()
    }

    /// Sends an item into the channel.
    ///
    /// # Errors
    ///
    /// If the receiver has disconnected (does not exist anymore), then `Err(SendError)` be returned
    /// with the given `item`.
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        // Calculate how many permits we need, and wait until we can acquire all of them.
        let (limit, count, permits_required) = self.calc_required_permits(&item);
        let in_use = limit.saturating_sub(self.available_capacity());
        match self
            .inner
            .limiter
            .clone()
            .acquire_many_owned(permits_required)
            .await
        {
            Ok(permits) => {
                self.inner.send_with_permit(in_use + count, permits, item);
                trace!("Sent item.");
                Ok(())
            }
            Err(_) => Err(SendError(item)),
        }
    }

    /// Attempts to send an item into the channel.
    ///
    /// # Errors
    ///
    /// If the receiver has disconnected (does not exist anymore), then
    /// `Err(TrySendError::Disconnected)` be returned with the given `item`. If the channel has
    /// insufficient capacity for the item, then `Err(TrySendError::InsufficientCapacity)` will be
    /// returned with the given `item`.
    ///
    /// # Panics
    ///
    /// Will panic if adding ack amount overflows.
    pub fn try_send(&mut self, item: T) -> Result<(), TrySendError<T>> {
        // Calculate how many permits we need, and try to acquire them all without waiting.
        let (limit, count, permits_required) = self.calc_required_permits(&item);
        let in_use = limit.saturating_sub(self.available_capacity());
        match self
            .inner
            .limiter
            .clone()
            .try_acquire_many_owned(permits_required)
        {
            Ok(permits) => {
                self.inner.send_with_permit(in_use + count, permits, item);
                trace!("Attempt to send item succeeded.");
                Ok(())
            }
            Err(TryAcquireError::NoPermits) => Err(TrySendError::InsufficientCapacity(item)),
            Err(TryAcquireError::Closed) => Err(TrySendError::Disconnected(item)),
        }
    }
}

impl<T> Clone for LimitedSender<T> {
    fn clone(&self) -> Self {
        self.sender_count.fetch_add(1, Ordering::SeqCst);

        Self {
            inner: self.inner.clone(),
            sender_count: Arc::clone(&self.sender_count),
        }
    }
}

impl<T> Drop for LimitedSender<T> {
    fn drop(&mut self) {
        // If we're the last sender to drop, close the semaphore on our way out the door.
        if self.sender_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.inner.limiter.close();
            self.inner.read_waker.notify_one();
        }
    }
}

#[derive(Debug)]
pub struct LimitedReceiver<T> {
    inner: Inner<T>,
}

impl<T: Send + 'static> LimitedReceiver<T> {
    /// Gets the number of items that this channel could accept.
    pub fn available_capacity(&self) -> usize {
        self.inner.limiter.available_permits()
    }

    pub async fn next(&mut self) -> Option<T> {
        loop {
            if let Some((_permit, item)) = self.inner.data.pop() {
                return Some(item);
            }

            // There wasn't an item for us to pop, so see if the channel is actually closed.  If so,
            // then it's time for us to close up shop as well.
            if self.inner.limiter.is_closed() {
                return None;
            }

            // We're not closed, so we need to wait for a writer to tell us they made some
            // progress.  This might end up being a spurious wakeup since `Notify` will
            // store a wake-up if there are no waiters, but oh well.
            self.inner.read_waker.notified().await;
        }
    }

    pub fn into_stream(self) -> Pin<Box<dyn Stream<Item = T> + Send>> {
        let mut receiver = self;
        Box::pin(stream! {
            while let Some(item) = receiver.next().await {
                yield item;
            }
        })
    }
}

impl<T> Drop for LimitedReceiver<T> {
    fn drop(&mut self) {
        // Notify senders that the channel is now closed by closing the semaphore.  Any pending
        // acquisitions will be awoken and notified that the semaphore is closed, and further new
        // sends will immediately see the semaphore is closed.
        self.inner.limiter.close();
    }
}

pub fn limited<T: InMemoryBufferable + fmt::Debug>(
    limit: MemoryBufferSize,
    metric_metadata: Option<ChannelMetricMetadata>,
) -> (LimitedSender<T>, LimitedReceiver<T>) {
    let inner = Inner::new(limit, metric_metadata);

    let sender = LimitedSender {
        inner: inner.clone(),
        sender_count: Arc::new(AtomicUsize::new(1)),
    };
    let receiver = LimitedReceiver { inner };

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use tokio_test::{assert_pending, assert_ready, task::spawn};
    use vector_common::byte_size_of::ByteSizeOf;

    use super::{ChannelMetricMetadata, limited};
    use crate::{
        MemoryBufferSize,
        test::MultiEventRecord,
        topology::{channel::limited_queue::SendError, test_util::Sample},
    };

    #[tokio::test]
    async fn send_receive() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(2).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(2, tx.available_capacity());

        let msg = Sample::new(42);

        // Create our send and receive futures.
        let mut send = spawn(async { tx.send(msg).await });

        let mut recv = spawn(async { rx.next().await });

        // Nobody should be woken up.
        assert!(!send.is_woken());
        assert!(!recv.is_woken());

        // Try polling our receive, which should be pending because we haven't anything yet.
        assert_pending!(recv.poll());

        // We should immediately be able to complete a send as there is available capacity.
        assert_eq!(Ok(()), assert_ready!(send.poll()));

        // Now our receive should have been woken up, and should immediately be ready.
        assert!(recv.is_woken());
        assert_eq!(Some(Sample::new(42)), assert_ready!(recv.poll()));
    }

    #[tokio::test]
    async fn records_utilization_on_send() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(2).unwrap());
        let (mut tx, mut rx) = limited(
            limit,
            Some(ChannelMetricMetadata::new("test_channel", None)),
        );

        let metrics = tx.inner.metrics.as_ref().unwrap().recorded_values.clone();

        tx.send(Sample::new(1)).await.expect("send should succeed");
        assert_eq!(metrics.lock().unwrap().last().copied(), Some(1));

        let _ = rx.next().await;
    }

    #[test]
    fn test_limiting_by_byte_size() {
        let max_elements = 10;
        let msg = Sample::new_with_heap_allocated_values(50);
        let msg_size = msg.allocated_bytes();
        let max_allowed_bytes = msg_size * max_elements;

        // With this configuration a maximum of exactly 10 messages can fit in the channel
        let limit = MemoryBufferSize::MaxSize(NonZeroUsize::new(max_allowed_bytes).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(max_allowed_bytes, tx.available_capacity());

        // Send 10 messages into the channel, filling it
        for _ in 0..10 {
            let msg_clone = msg.clone();
            let mut f = spawn(async { tx.send(msg_clone).await });
            assert_eq!(Ok(()), assert_ready!(f.poll()));
        }
        // With the 10th message in the channel no space should be left
        assert_eq!(0, tx.available_capacity());

        // Attemting to produce one more then the max capacity should block
        let mut send_final = spawn({
            let msg_clone = msg.clone();
            async { tx.send(msg_clone).await }
        });
        assert_pending!(send_final.poll());

        // Read all data from the channel, assert final states are as expected
        for _ in 0..10 {
            let mut f = spawn(async { rx.next().await });
            let value = assert_ready!(f.poll());
            assert_eq!(value.allocated_bytes(), msg_size);
        }
        // Channel should have no more data
        let mut recv = spawn(async { rx.next().await });
        assert_pending!(recv.poll());
    }

    #[test]
    fn sender_waits_for_more_capacity_when_none_available() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(1).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(1, tx.available_capacity());

        let msg1 = Sample::new(42);
        let msg2 = Sample::new(43);

        // Create our send and receive futures.
        let mut send1 = spawn(async { tx.send(msg1).await });

        let mut recv1 = spawn(async { rx.next().await });

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
        let mut send2 = spawn(async { tx.send(msg2).await });

        assert!(!send2.is_woken());
        assert_pending!(send2.poll());

        // Now if we receive the item, our second send should be woken up and be able to send in.
        assert_eq!(Some(Sample::new(42)), assert_ready!(recv1.poll()));
        drop(recv1);

        // Since the second send was already waiting for permits, the semaphore returns them
        // directly to our waiting send, which should now be woken up and able to complete:
        assert_eq!(0, rx.available_capacity());
        assert!(send2.is_woken());

        let mut recv2 = spawn(async { rx.next().await });
        assert_pending!(recv2.poll());

        assert_eq!(Ok(()), assert_ready!(send2.poll()));
        drop(send2);

        assert_eq!(0, tx.available_capacity());

        // And the final receive to get our second send:
        assert!(recv2.is_woken());
        assert_eq!(Some(Sample::new(43)), assert_ready!(recv2.poll()));

        assert_eq!(1, tx.available_capacity());
    }

    #[test]
    fn sender_waits_for_more_capacity_when_partial_available() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(7).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(7, tx.available_capacity());

        let msgs1 = vec![
            MultiEventRecord::new(1),
            MultiEventRecord::new(2),
            MultiEventRecord::new(3),
        ];
        let msg2 = MultiEventRecord::new(4);

        // Create our send and receive futures.
        let mut small_sends = spawn(async {
            for msg in msgs1.clone() {
                tx.send(msg).await?;
            }

            Ok::<_, SendError<MultiEventRecord>>(())
        });

        let mut recv1 = spawn(async { rx.next().await });

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
        let mut send2 = spawn(tx.send(msg2.clone()));

        assert!(!send2.is_woken());
        assert_pending!(send2.poll());

        // Now if we receive the first item, our second send should be woken up but still not able
        // to send.
        assert_eq!(Some(&msgs1[0]), assert_ready!(recv1.poll()).as_ref());
        drop(recv1);

        // Callers waiting to acquire permits have the permits immediately transfer to them when one
        // (or more) are released, so we expect this to be zero until we send and then read the
        // third item.
        assert_eq!(0, rx.available_capacity());

        // We don't get woken up until all permits have been acquired.
        assert!(!send2.is_woken());

        // Our second read should unlock enough available capacity for the second send once complete.
        let mut recv2 = spawn(async { rx.next().await });
        assert!(!recv2.is_woken());
        assert_eq!(Some(&msgs1[1]), assert_ready!(recv2.poll()).as_ref());
        drop(recv2);

        assert_eq!(0, rx.available_capacity());

        assert!(send2.is_woken());
        assert_eq!(Ok(()), assert_ready!(send2.poll()));

        // And just make sure we see those last two sends.
        let mut recv3 = spawn(async { rx.next().await });
        assert!(!recv3.is_woken());
        assert_eq!(Some(&msgs1[2]), assert_ready!(recv3.poll()).as_ref());
        drop(recv3);

        assert_eq!(3, rx.available_capacity());

        let mut recv4 = spawn(async { rx.next().await });
        assert!(!recv4.is_woken());
        assert_eq!(Some(msg2), assert_ready!(recv4.poll()));
        drop(recv4);

        assert_eq!(7, rx.available_capacity());
    }

    #[test]
    fn empty_receiver_returns_none_when_last_sender_drops() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(1).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(1, tx.available_capacity());

        let tx2 = tx.clone();
        let msg = Sample::new(42);

        // Create our send and receive futures.
        let mut send = spawn(async { tx.send(msg).await });

        let mut recv = spawn(async { rx.next().await });

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
        assert_eq!(Some(Sample::new(42)), assert_ready!(recv.poll()));
        drop(recv);

        let mut recv2 = spawn(async { rx.next().await });
        assert!(!recv2.is_woken());
        assert_eq!(None, assert_ready!(recv2.poll()));
    }

    #[test]
    fn receiver_returns_none_once_empty_when_last_sender_drops() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(1).unwrap());
        let (tx, mut rx) = limited::<Sample>(limit, None);

        assert_eq!(1, tx.available_capacity());

        let tx2 = tx.clone();

        // Create our receive future.
        let mut recv = spawn(async { rx.next().await });

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
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(1).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(1, tx.available_capacity());

        let msg = MultiEventRecord::new(2);

        // Create our send and receive futures.
        let mut send = spawn(async { tx.send(msg.clone()).await });

        let mut recv = spawn(async { rx.next().await });

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
        assert_eq!(Some(msg), assert_ready!(recv.poll()));
        drop(recv);

        assert_eq!(1, rx.available_capacity());
    }

    #[test]
    fn oversized_send_allowed_when_partial_capacity() {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(2).unwrap());
        let (mut tx, mut rx) = limited(limit, None);

        assert_eq!(2, tx.available_capacity());

        let msg1 = MultiEventRecord::new(1);
        let msg2 = MultiEventRecord::new(3);

        // Create our send future.
        let mut send = spawn(async { tx.send(msg1.clone()).await });

        // Nobody should be woken up.
        assert!(!send.is_woken());

        // We should immediately be able to complete our send, which will only use up a single slot.
        assert_eq!(Ok(()), assert_ready!(send.poll()));
        drop(send);

        assert_eq!(1, tx.available_capacity());

        // Now we'll trigger another send which has an oversized item.  It shouldn't be able to send
        // until all permits are available.
        let mut send2 = spawn(async { tx.send(msg2.clone()).await });

        assert!(!send2.is_woken());
        assert_pending!(send2.poll());

        assert_eq!(0, rx.available_capacity());

        // Now do a receive which should return the one consumed slot, essentially allowing all
        // permits to be acquired by the blocked send.
        let mut recv = spawn(async { rx.next().await });
        assert!(!recv.is_woken());
        assert!(!send2.is_woken());

        assert_eq!(Some(msg1), assert_ready!(recv.poll()));
        drop(recv);

        assert_eq!(0, rx.available_capacity());

        // Now our blocked send should be able to proceed, and we should be able to read back the
        // item.
        assert_eq!(Ok(()), assert_ready!(send2.poll()));
        drop(send2);

        assert_eq!(0, tx.available_capacity());

        let mut recv2 = spawn(async { rx.next().await });
        assert_eq!(Some(msg2), assert_ready!(recv2.poll()));

        assert_eq!(2, tx.available_capacity());
    }
}
