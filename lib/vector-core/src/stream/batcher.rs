use bimap::BiHashMap;
use tokio_util::time::delay_queue::Key;
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
use tokio_util::time::DelayQueue;
use twox_hash::XxHash64;

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
pub struct Batcher<St, Prt>
where
    Prt: Partitioner,
{
    /// The queue of pending batch expirations
    expirations: DelayQueue<Prt::Key>,
    /// The map of batch expiration keys to the partition key for the batch
    expiration_map: BiHashMap<Key, Prt::Key>,
    /// The timeout for an individual batch
    batch_timeout: Duration,
    /// The maximum number of items that are allowed per-batch
    batch_item_limit: usize,
    /// The total number of bytes a single batch in this struct is allowed to
    /// hold.
    batch_allocation_limit: usize,
    /// The store of live batches. Note that the key here is an option type,
    /// on account of the interface of `Prt`.
    batches: HashMap<Prt::Key, Batch<Prt::Item>, BuildHasherDefault<XxHash64>>,
    /// The store of 'closed' batches. When this is not empty it will be
    /// preferentially flushed prior to consuming any new items from the
    /// underlying stream.
    closed_batches: Vec<(Prt::Key, Vec<Prt::Item>)>,
    /// The partitioner for this `Batcher`
    partitioner: Prt,
    #[pin]
    /// The stream this `Batcher` wraps
    stream: St,
}

impl<St, Prt> Batcher<St, Prt>
where
    St: Stream + Unpin,
    Prt: Partitioner + Unpin,
{
    pub fn new(
        stream: St,
        partitioner: Prt,
        batch_timeout: Duration,
        batch_item_limit: NonZeroUsize,
        batch_allocation_limit: Option<NonZeroUsize>,
    ) -> Self {
        Self {
            batch_item_limit: batch_item_limit.get(),
            batch_allocation_limit: batch_allocation_limit
                .map_or(usize::max_value(), NonZeroUsize::get),
            batches: HashMap::default(),
            closed_batches: Vec::default(),
            batch_timeout,
            expirations: DelayQueue::new(),
            expiration_map: BiHashMap::new(),
            partitioner,
            stream,
        }
    }
}

impl<St, Prt> Stream for Batcher<St, Prt>
where
    St: Stream<Item = Prt::Item> + Unpin,
    Prt: Partitioner + Unpin,
    Prt::Key: Clone,
    Prt::Item: ByteSizeOf,
{
    type Item = (Prt::Key, Vec<Prt::Item>);

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
                Poll::Pending => match this.expirations.poll_expired(cx) {
                    // Unlike normal streams, `DelayQueue` can return `None` here but still be
                    // usable later if more entries are added.
                    Poll::Pending | Poll::Ready(None) => return Poll::Pending,
                    Poll::Ready(Some(expiration)) => match expiration {
                        // We shouldn't really ever hit this error arm, as `DelayQueue` doesn't
                        // actually return ever return an error, but it's part of the type signature
                        // so we must abide.
                        Err(e) => error!("caught unexpected error while polling for expired batches: {}", e),
                        Ok(expiration) => {
                            let (_, item_key) = this.expiration_map.remove_by_left(&expiration.key())
                                .expect("batch should not expire if it does not exist");
                            let batch = this.batches.remove(&item_key)
                                .expect("batch should exist if it is set to expire");
                            this.closed_batches.push((item_key, batch.into_inner()));
                            
                            continue
                        },
                    }
                },
                Poll::Ready(None) => {
                    // Now that the underlying stream is closed, we need to clear out our batches,
                    // including all expiration entries. If we had any batches to hand over, we have
                    // to continue looping so the caller can drain them all before we finish.
                    if !this.batches.is_empty() {
                        this.expirations.clear();
                        this.expiration_map.clear();
                        this.closed_batches.extend(
                            this.batches
                                .drain()
                                .map(|(key, batch)| (key, batch.into_inner())),
                        );
                        continue
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
                            let new_batch = Batch::new(item_limit, alloc_limit).with(item);
                            let batch = mem::replace(batch, new_batch);

                            // The batch for this partition key was set to expire, but now it's
                            // overflowed and must be pushed out, so now we reset the batch
                            // timeout.
                            let expiration_key = this.expiration_map.get_by_right(&item_key)
                                .expect("expiration mapping should always exist for batch");
                            this.expirations.reset(expiration_key, *this.batch_timeout);

                            this.closed_batches.push((item_key, batch.into_inner()));
                        }
                    } else {
                        // We have no batch yet for this partition key, so create one and create the
                        // expiration entries as well.  This allows the batch to expire before
                        // filling up, and vise versa.
                        let batch = Batch::new(item_limit, alloc_limit).with(item);
                        this.batches.insert(item_key.clone(), batch);

                        let expiration_key = this.expirations.insert(item_key.clone(), *this.batch_timeout);
                        if let Err(_) = this.expiration_map.insert_no_overwrite(expiration_key, item_key) {
                            panic!("there shoud never be an existing expiration map entry for a brand new batch");
                        }
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
    use tokio::runtime::Runtime;
    use std::collections::HashMap;
    use std::num::{NonZeroU8, NonZeroUsize};
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

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
        fn partition(&self, item: &Self::Item) -> Self::Key {
            let key = *item % u64::from(self.key_space.get());
            key as Self::Key
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

            let timeout_limit = Duration::from_nanos(1);
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let batcher = Batcher::new(&mut stream, partitioner, timeout_limit,
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

            let mut stream = stream::iter(stream.into_iter());
            let timeout_limit = Duration::from_nanos(1);
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = Batcher::new(&mut stream, partitioner, timeout_limit,
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
    ) -> HashMap<u8, Vec<u64>> {
        let mut map = stream
            .into_iter()
            .map(|item| {
                let key = partitioner.partition(&item);
                (key, item)
            })
            .fold(
                HashMap::default(),
                |mut acc: HashMap<u8, Vec<u64>>, (key, item)| {
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

            let mut stream = stream::iter(stream.into_iter());
            let timeout_limit = Duration::from_nanos(1);
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = Batcher::new(&mut stream, partitioner, timeout_limit,
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

            let total_items = stream.len();
            let mut stream = stream::iter(stream.into_iter());
            let timeout_limit = Duration::from_nanos(1);
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = Batcher::new(&mut stream, partitioner, timeout_limit,
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
