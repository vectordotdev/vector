use crate::partition::Partitioner;
use crate::time::Timer;
use crate::ByteSizeOf;
use futures::stream::Stream;
use pin_project::pin_project;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::mem;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{interval, Interval, MissedTickBehavior};
use twox_hash::XxHash64;

/// A [`Timer`] for [`Batch`]
///
/// A `Batch` implementation must be rigged to flush every so often on a
/// timer. Absent any particular concerns this timer will serve most
/// purposes. It stores an internal notion of when it was started and how "wide"
/// away from that start time can drift before the timer has elapsed.
pub struct BatcherTimer {
    interval: Interval,
}

impl BatcherTimer {
    #[allow(dead_code)]
    pub fn new(period: Duration) -> Self {
        let mut interval = interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        Self { interval }
    }
}

impl Timer for BatcherTimer {
    fn poll_elapsed(&mut self, cx: &mut Context) -> Poll<()> {
        self.interval.poll_tick(cx).map(|_| ())
    }
}

/// A batch for use by `Batcher`
///
/// This structure is a private implementation detail that simplifies the
/// implementation of `Batcher`. It is the actual store of items that come
/// through the stream manipulated by `Batcher` plus limit information to signal
/// when the `Batch` is full.
struct Batch<I> {
    /// The total number of `I` bytes stored, does not any overhead in this
    /// structure.
    allocated_bytes: usize,
    /// The maximum number of elements allowed in this structure.
    element_limit: usize,
    /// The maximum number of allocated bytes (not including overhead) allowed
    /// in this structure.
    allocation_limit: usize,
    /// The store of `I` elements.
    elements: Vec<I>,
}

impl<I> ByteSizeOf for Batch<I> {
    fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }
}

impl<I> Batch<I>
where
    I: ByteSizeOf,
{
    /// Create a new Batch instance
    ///
    /// Creates a new batch instance with specific element and allocation
    /// limits. The element limit is a maximum cap on the number of `I`
    /// instances. The allocation limit is a soft-max on the number of allocated
    /// bytes stored in this batch, not taking into account overhead from this
    /// structure itself. Caller is responsible for ensuring that `I` will fit
    /// inside the allocation limit.
    ///
    /// # Panics
    ///
    /// This function will panic if the allocation limit will not store at least
    /// 1 instance of `I`, as measured by `mem::size_of`.
    fn new(element_limit: usize, allocation_limit: usize) -> Self {
        assert!(allocation_limit >= mem::size_of::<I>());
        Self {
            allocated_bytes: 0,
            element_limit,
            allocation_limit,
            elements: Vec::with_capacity(element_limit),
        }
    }

    /// Unconditionally insert an element into the batch
    ///
    /// This function is similar to `push` except that the caller does not need
    /// to call `has_space` prior to calling this and it will never
    /// panic. Intended to be used only when insertion must not fail.
    fn with(mut self, value: I) -> Self {
        self.allocated_bytes += value.size_of();
        self.elements.push(value);
        self
    }

    /// Decompose the batch
    ///
    /// Called by the user when they want to get at the internal store of
    /// items. Returns a tuple, the first element being the allocated size of
    /// stored items and the second the store of items.
    fn into_inner(self) -> Vec<I> {
        self.elements
    }

    /// Whether the batch has space for a new item
    ///
    /// This function returns true of there is space both in terms of item count
    /// and byte count for the given item, false otherwise.
    fn has_space(&self, value: &I) -> bool {
        let too_many_elements = self.elements.len() + 1 > self.element_limit;
        let too_many_bytes = self.allocated_bytes + value.size_of() > self.allocation_limit;
        !(too_many_elements || too_many_bytes)
    }

    /// Push an element into the batch
    ///
    /// This function pushes an element into the batch. Callers must be sure to
    /// call `has_space` prior to calling this function and receive a positive
    /// result.
    ///
    /// # Panics
    ///
    /// This function will panic if there is not sufficient space in the batch
    /// for a new element to be inserted.
    fn push(&mut self, value: I) {
        assert!(self.has_space(&value));
        self.allocated_bytes += value.size_of();
        self.elements.push(value);
    }
}

#[pin_project]
pub struct Batcher<St, Prt, T>
where
    Prt: Partitioner,
{
    /// The timer to maintain periodic flushes from this `Batcher`
    timer: T,
    /// The maximum number of items that are allowed per-batch
    batch_item_limit: usize,
    /// The total number of bytes a single batch in this struct is allowed to
    /// hold.
    batch_allocation_limit: usize,
    /// The store of live batches. Note that the key here is an option type,
    /// on account of the interface of `Prt`.
    batches: HashMap<Option<Prt::Key>, Batch<Prt::Item>, BuildHasherDefault<XxHash64>>,
    /// The store of 'closed' batches. When this is not empty it will be
    /// preferentially flushed prior to consuming any new items from the
    /// underlying stream.
    closed_batches: Vec<(Option<Prt::Key>, Vec<Prt::Item>)>,
    /// The partitioner for this `Batcher`
    partitioner: Prt,
    #[pin]
    /// The stream this `Batcher` wraps
    stream: St,
}

impl<St, Prt, T> Batcher<St, Prt, T>
where
    St: Stream + Unpin,
    Prt: Partitioner + Unpin,
{
    pub fn new(
        stream: St,
        partitioner: Prt,
        timer: T,
        batch_item_limit: NonZeroUsize,
        batch_allocation_limit: Option<NonZeroUsize>,
    ) -> Self {
        Self {
            batch_item_limit: batch_item_limit.get(),
            batch_allocation_limit: batch_allocation_limit
                .map_or(usize::max_value(), NonZeroUsize::get),
            batches: HashMap::default(),
            closed_batches: Vec::default(),
            timer,
            partitioner,
            stream,
        }
    }
}

impl<St, Prt, T> Stream for Batcher<St, Prt, T>
where
    St: Stream<Item = Prt::Item> + Unpin,
    Prt: Partitioner + Unpin,
    Prt::Item: ByteSizeOf,
    T: Timer,
{
    type Item = (Option<Prt::Key>, Vec<Prt::Item>);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            if !this.closed_batches.is_empty() {
                return Poll::Ready(this.closed_batches.pop());
            }
            match this.stream.as_mut().poll_next(cx) {
                Poll::Pending => match this.timer.poll_elapsed(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(()) => {
                        this.closed_batches.extend(
                            this.batches
                                .drain()
                                .map(|(key, batch)| (key, batch.into_inner())),
                        );
                        continue;
                    }
                },
                Poll::Ready(None) => {
                    // Now that the underlying stream is closed we need to clear
                    // out our batches.
                    if !this.batches.is_empty() {
                        this.closed_batches.extend(
                            this.batches
                                .drain()
                                .map(|(key, batch)| (key, batch.into_inner())),
                        );
                        continue;
                    }
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(item)) => {
                    let item_key = this.partitioner.partition(&item);
                    let item_limit: usize = *this.batch_item_limit;
                    let alloc_limit: usize = *this.batch_allocation_limit;

                    if let Some(batch) = this.batches.get_mut(&item_key) {
                        if batch.has_space(&item) {
                            // When there's space in the partition batch just
                            // push the item in and loop back around.
                            batch.push(item);
                        } else {
                            match this.timer.poll_elapsed(cx) {
                                Poll::Pending => {
                                    // There's no space in the partition batch
                                    // but the timer hasn't fired yet. Swap out
                                    // the existing, full partition batch for a
                                    // new one, push the old one into
                                    // closed_batches.
                                    let nb = Batch::new(item_limit, alloc_limit).with(item);
                                    let batch = mem::replace(batch, nb);
                                    this.closed_batches.push((item_key, batch.into_inner()));
                                }
                                Poll::Ready(()) => {
                                    // The global timer has elapsed. Close all
                                    // batches, including the one we have a
                                    // mutable ref to, and then insert a brand
                                    // new batch with `item` in it.
                                    this.closed_batches.extend(
                                        this.batches
                                            .drain()
                                            .map(|(key, batch)| (key, batch.into_inner())),
                                    );
                                    // With all batches closed put the item into
                                    // a new batch.
                                    let batch = Batch::new(item_limit, alloc_limit).with(item);
                                    this.batches.insert(item_key, batch);
                                }
                            }
                        }
                    } else {
                        let batch = Batch::new(item_limit, alloc_limit).with(item);
                        this.batches.insert(item_key, batch);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::partition::Partitioner;
    use crate::stream::batcher::Batcher;
    use crate::time::Timer;
    use futures::task::noop_waker;
    use futures::{stream, Stream};
    use pin_project::pin_project;
    use proptest::prelude::*;
    use std::collections::HashMap;
    use std::num::{NonZeroU8, NonZeroUsize};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[derive(Debug)]
    /// A test timer
    ///
    /// This timer implements `Timer` and is rigged up in such a way that it
    /// doesn't _actually_ tell time but instead uses a set of canned responses
    /// for whether deadlines have elapsed or not. This allows us to include the
    /// notion of time in our property tests below.
    struct TestTimer {
        responses: Vec<Poll<()>>,
    }

    impl TestTimer {
        fn new(responses: Vec<Poll<()>>) -> Self {
            Self { responses }
        }
    }

    impl Timer for TestTimer {
        fn poll_elapsed(&mut self, _cx: &mut Context) -> Poll<()> {
            self.responses.pop().unwrap_or(Poll::Pending)
        }
    }

    fn arb_timer() -> impl Strategy<Value = TestTimer> {
        Vec::<bool>::arbitrary()
            .prop_map(|v| {
                v.into_iter()
                    .map(|i| if i { Poll::Pending } else { Poll::Ready(()) })
                    .collect()
            })
            .prop_map(TestTimer::new)
    }

    #[pin_project]
    #[derive(Debug)]
    /// A test partitioner
    ///
    /// This partitioner is nothing special. It has a large-ish key space but
    /// not so large that we'll never see batches accumulate properly.
    struct TestPartitioner {
        key_space: NonZeroU8,
    }

    impl Partitioner for TestPartitioner {
        type Item = u64;
        type Key = u8;

        #[allow(clippy::cast_possible_truncation)]
        fn partition(&self, item: &Self::Item) -> Option<Self::Key> {
            let key = *item % u64::from(self.key_space.get());
            Some(key as Self::Key)
        }
    }

    fn arb_partitioner() -> impl Strategy<Value = TestPartitioner> {
        (1..u8::max_value(),).prop_map(|(ks,)| TestPartitioner {
            key_space: NonZeroU8::new(ks).unwrap(),
        })
    }

    proptest! {
        #[test]
        fn size_hint_eq(stream: Vec<u64>,
                        item_limit in 1..u16::max_value(),
                        allocation_limit in 8..128,
                        partitioner in arb_partitioner(),
                        timer in arb_timer()) {
            // Asserts that the size hint of the batcher stream is the same as
            // that of the internal stream. In the future we may want to produce
            // a tighter bound -- since batching will reduce some streams -- but
            // this is the worst case where every incoming item maps to a unique
            // key.
            let mut stream = stream::iter(stream.into_iter());
            let stream_size_hint = stream.size_hint();

            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let batcher = Batcher::new(&mut stream, partitioner, timer,
                                       item_limit, Some(allocation_limit));
            let batcher_size_hint = batcher.size_hint();

            assert_eq!(stream_size_hint, batcher_size_hint);
        }
    }

    proptest! {
        #[test]
        fn batch_item_size_leq_limit(stream: Vec<u64>,
                                     item_limit in 1..u16::max_value(),
                                     allocation_limit in 8..128,
                                     partitioner in arb_partitioner(),
                                     timer in arb_timer()) {
            // Asserts that for every received batch the size is always less
            // than the expected limit.
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);

            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut stream = stream::iter(stream.into_iter());
            let mut batcher = Batcher::new(&mut stream, partitioner, timer,
                                           item_limit, Some(allocation_limit));
            let mut pin = Pin::new(&mut batcher);

            loop {
                match pin.as_mut().poll_next(&mut cx) {
                    Poll::Pending => {}
                    Poll::Ready(None) => {
                        break;
                    }
                    Poll::Ready(Some((_, batch))) => {
                        debug_assert!(
                            batch.len() <= item_limit.get(),
                            "{} < {}",
                            batch.len(),
                            item_limit.get()
                        );
                    }
                }
            }
        }
    }

    /// Separates a stream into partitions
    ///
    /// This function separates a stream into partitions, preserving the order
    /// of the items in reverse. This allows for efficient popping to compare
    /// ordering of receipt.
    fn separate_partitions(
        stream: Vec<u64>,
        partitioner: &TestPartitioner,
    ) -> HashMap<Option<u8>, Vec<u64>> {
        let mut map = stream
            .into_iter()
            .map(|item| {
                let key = partitioner.partition(&item);
                (key, item)
            })
            .fold(
                HashMap::default(),
                |mut acc: HashMap<Option<u8>, Vec<u64>>, (key, item)| {
                    let arr: &mut Vec<u64> = acc.entry(key).or_insert_with(Vec::default);
                    arr.push(item);
                    acc
                },
            );
        for part in map.values_mut() {
            part.reverse();
        }
        map
    }

    proptest! {
        #[test]
        fn batch_does_not_reorder(stream: Vec<u64>,
                                  item_limit in 1..u16::max_value(),
                                  allocation_limit in 8..128,
                                  partitioner in arb_partitioner(),
                                  timer in arb_timer()) {
            // Asserts that for every received batch received the elements in
            // the batch are not reordered within a batch. No claim is made on
            // when batches themselves will issue, batch sizes etc.
            let mut partitions = separate_partitions(stream.clone(), &partitioner);

            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);

            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let mut stream = stream::iter(stream.into_iter());
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = Batcher::new(&mut stream, partitioner, timer,
                                           item_limit, Some(allocation_limit));
            let mut pin = Pin::new(&mut batcher);

            loop {
                match pin.as_mut().poll_next(&mut cx) {
                    Poll::Pending => {}
                    Poll::Ready(None) => {
                        break;
                    }
                    Poll::Ready(Some((key, actual_batch))) => {
                        let expected_partition = partitions
                            .get_mut(&key)
                            .expect("impossible situation");
                        for item in actual_batch {
                            assert_eq!(item, expected_partition.pop().unwrap());
                        }
                    }
                }
            }
            for v in partitions.values() {
                assert!(v.is_empty());
            }
        }
    }

    proptest! {
        #[test]
        fn batch_does_not_lose_items(stream: Vec<u64>,
                                     item_limit in 1..u16::max_value(),
                                     allocation_limit in 8..128,
                                     partitioner in arb_partitioner(),
                                     timer in arb_timer()) {
            // Asserts that for every received batch the sum of all batch sizes
            // equals the number of stream elements.
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);

            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let total_items = stream.len();
            let mut stream = stream::iter(stream.into_iter());
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = Batcher::new(&mut stream, partitioner, timer,
                                           item_limit, Some(allocation_limit));
            let mut pin = Pin::new(&mut batcher);

            let mut observed_items = 0;
            loop {
                match pin.as_mut().poll_next(&mut cx) {
                    Poll::Pending => {}
                    Poll::Ready(None) => {
                        // inner stream has shut down, ensure we passed every
                        // item through the batch
                        assert_eq!(observed_items, total_items);
                        break;
                    }
                    Poll::Ready(Some((_, batch))) => {
                        observed_items += batch.len();
                        assert!(observed_items <= total_items);
                    }
                }
            }
        }
    }
}
