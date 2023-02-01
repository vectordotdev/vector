use std::{
    cmp,
    collections::HashMap,
    hash::{BuildHasherDefault, Hash},
    mem,
    num::NonZeroUsize,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

use futures::stream::{Fuse, Stream, StreamExt};
use pin_project::pin_project;
use tokio_util::time::{delay_queue::Key, DelayQueue};
use twox_hash::XxHash64;

use crate::{
    partition::Partitioner,
    stream::batcher::{
        config::BatchConfigParts,
        data::BatchReduce,
        limiter::{ByteSizeOfItemSize, ItemBatchSize, SizeLimit},
    },
    time::KeyedTimer,
    ByteSizeOf,
};

/// A `KeyedTimer` based on `DelayQueue`.
pub struct ExpirationQueue<K> {
    /// The timeout to give each new key entry
    timeout: Duration,
    /// The queue of expirations
    expirations: DelayQueue<K>,
    /// The Key -> K mapping, allows for resets
    expiration_map: HashMap<K, Key>,
}

impl<K> ExpirationQueue<K> {
    /// Creates a new `ExpirationQueue`.
    ///
    /// `timeout` is used for all insertions and resets.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            expirations: DelayQueue::new(),
            expiration_map: HashMap::default(),
        }
    }

    /// The number of current subtimers in the queue.
    ///
    /// Includes subtimers which have expired but have not yet been removed via
    /// calls to `poll_expired`.
    pub fn len(&self) -> usize {
        self.expirations.len()
    }

    /// Returns `true` if the queue has a length of 0.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<K> KeyedTimer<K> for ExpirationQueue<K>
where
    K: Eq + Hash + Clone,
{
    fn clear(&mut self) {
        self.expirations.clear();
        self.expiration_map.clear();
    }

    fn insert(&mut self, item_key: K) {
        if let Some(expiration_key) = self.expiration_map.get(&item_key) {
            // We already have an expiration entry for this item key, so
            // just reset the expiration.
            self.expirations.reset(expiration_key, self.timeout);
        } else {
            // This is a yet-unseen item key, so create a new expiration
            // entry.
            let expiration_key = self.expirations.insert(item_key.clone(), self.timeout);
            assert!(self
                .expiration_map
                .insert(item_key, expiration_key)
                .is_none());
        }
    }

    fn poll_expired(&mut self, cx: &mut Context) -> Poll<Option<K>> {
        match ready!(self.expirations.poll_expired(cx)) {
            // No expirations yet.
            None => Poll::Ready(None),
            Some(expiration) => {
                // An item has expired, so remove it from the map and return
                // it.
                assert!(self.expiration_map.remove(expiration.get_ref()).is_some());
                Poll::Ready(Some(expiration.into_inner()))
            }
        }
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
    /// Creates a new batch instance with specific element and allocation limits. The element limit
    /// is a maximum cap on the number of `I` instances. The allocation limit is a soft-max on the
    /// number of allocated bytes stored in this batch, not taking into account overhead from this
    /// structure itself.
    ///
    /// If `allocation_limit` is smaller than the size of `I` as reported by `std::mem::size_of`,
    /// then the allocation limit will be raised such that the batch can hold a single instance of
    /// `I`.  Likewise, `element_limit` will be raised such that it is always at least 1, ensuring
    /// that a new batch can be pushed into.
    fn new(element_limit: usize, allocation_limit: usize) -> Self {
        // SAFETY: `element_limit` is always non-zero because `BatcherSettings` can only be
        // constructed with `NonZeroUsize` versions of allocation limit/item limit.  `Batch` is also
        // only constructable via `Batcher`.

        // TODO: This may need to be reworked, because it's subtly wrong as-is.
        // ByteSizeOf::size_of() always returns the size of the type itself, plus any "allocated
        // bytes".  Thus, there are times when an item will be bigger than simply the size of the
        // type itself (aka mem::size_of::<I>()) and thus than type of item would never fit in a
        // batch where the `allocation_limit` is at or lower than the size of that item.
        //
        // We're counteracting this here by ensuring that the element limit is always at least 1.
        let allocation_limit = cmp::max(allocation_limit, mem::size_of::<I>());
        Self {
            allocated_bytes: 0,
            element_limit,
            allocation_limit,
            elements: Vec::with_capacity(128),
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

/// Controls the behavior of the batcher in terms of batch size and flush interval.
///
/// This is a temporary solution for pushing in a fixed settings structure so we don't have to worry
/// about misordering parameters and what not.  At some point, we will pull
/// `BatchConfig`/`BatchSettings`/`BatchSize` out of `vector` and move them into `vector_core`, and
/// make it more generalized. We can't do that yet, though, until we've converted all of the sinks
/// with their various specialized batch buffers.
#[derive(Copy, Clone, Debug)]
pub struct BatcherSettings {
    pub timeout: Duration,
    pub size_limit: usize,
    pub item_limit: usize,
}

impl BatcherSettings {
    pub const fn new(
        timeout: Duration,
        size_limit: NonZeroUsize,
        item_limit: NonZeroUsize,
    ) -> Self {
        BatcherSettings {
            timeout,
            size_limit: size_limit.get(),
            item_limit: item_limit.get(),
        }
    }

    /// A batcher config using the `ByteSizeOf` trait to determine batch sizes.
    /// The output is a  `Vec<T>`.
    pub fn into_byte_size_config<T: ByteSizeOf>(
        self,
    ) -> BatchConfigParts<SizeLimit<ByteSizeOfItemSize>, Vec<T>> {
        self.into_item_size_config(ByteSizeOfItemSize)
    }

    /// A batcher config using the `ItemBatchSize` trait to determine batch sizes.
    /// The output is a `Vec<T>`.
    pub fn into_item_size_config<T, I>(self, item_size: I) -> BatchConfigParts<SizeLimit<I>, Vec<T>>
    where
        I: ItemBatchSize<T>,
    {
        BatchConfigParts {
            batch_limiter: SizeLimit {
                batch_size_limit: self.size_limit,
                batch_item_limit: self.item_limit,
                current_size: 0,
                item_size_calculator: item_size,
            },
            batch_data: vec![],
            timeout: self.timeout,
        }
    }

    /// A batcher config using the `ItemBatchSize` trait to determine batch sizes.
    /// The output is built with the supplied reducer function.
    pub fn into_reducer_config<I, T, F, S>(
        self,
        item_size: I,
        reducer: F,
    ) -> BatchConfigParts<SizeLimit<I>, BatchReduce<F, S>>
    where
        I: ItemBatchSize<T>,
        F: FnMut(&mut S, T),
        S: Default,
    {
        BatchConfigParts {
            batch_limiter: SizeLimit {
                batch_size_limit: self.size_limit,
                batch_item_limit: self.item_limit,
                current_size: 0,
                item_size_calculator: item_size,
            },
            batch_data: BatchReduce::new(reducer),
            timeout: self.timeout,
        }
    }
}

#[pin_project]
pub struct PartitionedBatcher<St, Prt, KT>
where
    Prt: Partitioner,
{
    /// The total number of bytes a single batch in this struct is allowed to
    /// hold.
    batch_allocation_limit: usize,
    /// The maximum number of items that are allowed per-batch
    batch_item_limit: usize,
    /// The store of live batches. Note that the key here is an option type,
    /// on account of the interface of `Prt`.
    batches: HashMap<Prt::Key, Batch<Prt::Item>, BuildHasherDefault<XxHash64>>,
    /// The store of 'closed' batches. When this is not empty it will be
    /// preferentially flushed prior to consuming any new items from the
    /// underlying stream.
    closed_batches: Vec<(Prt::Key, Vec<Prt::Item>)>,
    /// The queue of pending batch expirations
    timer: KT,
    /// The partitioner for this `Batcher`
    partitioner: Prt,
    #[pin]
    /// The stream this `Batcher` wraps
    stream: Fuse<St>,
}

impl<St, Prt> PartitionedBatcher<St, Prt, ExpirationQueue<Prt::Key>>
where
    St: Stream<Item = Prt::Item>,
    Prt: Partitioner + Unpin,
    Prt::Key: Eq + Hash + Clone,
    Prt::Item: ByteSizeOf,
{
    pub fn new(stream: St, partitioner: Prt, settings: BatcherSettings) -> Self {
        Self {
            batch_allocation_limit: settings.size_limit,
            batch_item_limit: settings.item_limit,
            batches: HashMap::default(),
            closed_batches: Vec::default(),
            timer: ExpirationQueue::new(settings.timeout),
            partitioner,
            stream: stream.fuse(),
        }
    }
}

impl<St, Prt, KT> PartitionedBatcher<St, Prt, KT>
where
    St: Stream<Item = Prt::Item>,
    Prt: Partitioner + Unpin,
    Prt::Key: Eq + Hash + Clone,
    Prt::Item: ByteSizeOf,
{
    pub fn with_timer(
        stream: St,
        partitioner: Prt,
        timer: KT,
        batch_item_limit: NonZeroUsize,
        batch_allocation_limit: Option<NonZeroUsize>,
    ) -> Self {
        Self {
            batch_allocation_limit: batch_allocation_limit
                .map_or(usize::max_value(), NonZeroUsize::get),
            batch_item_limit: batch_item_limit.get(),
            batches: HashMap::default(),
            closed_batches: Vec::default(),
            timer,
            partitioner,
            stream: stream.fuse(),
        }
    }
}

impl<St, Prt, KT> Stream for PartitionedBatcher<St, Prt, KT>
where
    St: Stream<Item = Prt::Item>,
    Prt: Partitioner + Unpin,
    Prt::Key: Eq + Hash + Clone,
    Prt::Item: ByteSizeOf,
    KT: KeyedTimer<Prt::Key>,
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
                Poll::Pending => match this.timer.poll_expired(cx) {
                    // Unlike normal streams, `DelayQueue` can return `None`
                    // here but still be usable later if more entries are added.
                    Poll::Pending | Poll::Ready(None) => return Poll::Pending,
                    Poll::Ready(Some(item_key)) => {
                        let batch = this
                            .batches
                            .remove(&item_key)
                            .expect("batch should exist if it is set to expire");
                        this.closed_batches.push((item_key, batch.into_inner()));

                        continue;
                    }
                },
                Poll::Ready(None) => {
                    // Now that the underlying stream is closed, we need to
                    // clear out our batches, including all expiration
                    // entries. If we had any batches to hand over, we have to
                    // continue looping so the caller can drain them all before
                    // we finish.
                    if !this.batches.is_empty() {
                        this.timer.clear();
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
                            let new_batch = Batch::new(item_limit, alloc_limit).with(item);
                            let batch = mem::replace(batch, new_batch);

                            // The batch for this partition key was set to
                            // expire, but now it's overflowed and must be
                            // pushed out, so now we reset the batch timeout.
                            this.timer.insert(item_key.clone());

                            this.closed_batches.push((item_key, batch.into_inner()));
                        }
                    } else {
                        // We have no batch yet for this partition key, so
                        // create one and create the expiration entries as well.
                        // This allows the batch to expire before filling up,
                        // and vise versa.
                        let batch = Batch::new(item_limit, alloc_limit).with(item);
                        this.batches.insert(item_key.clone(), batch);
                        this.timer.insert(item_key);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::{HashMap, HashSet},
        num::{NonZeroU8, NonZeroUsize},
        pin::Pin,
        task::{Context, Poll},
        time::Duration,
    };

    use futures::{stream, Stream};
    use pin_project::pin_project;
    use proptest::prelude::*;
    use tokio::{pin, time::advance};

    use crate::{
        partition::Partitioner,
        stream::partitioned_batcher::{ExpirationQueue, PartitionedBatcher},
        time::KeyedTimer,
    };

    #[derive(Debug)]
    /// A test keyed timer
    ///
    /// This timer implements `KeyedTimer` and is rigged up in such a way that
    /// it doesn't _actually_ tell time but instead uses a set of canned
    /// responses for whether deadlines have elapsed or not. This allows us to
    /// include the notion of time in our property tests below.
    struct TestTimer {
        responses: Vec<Poll<Option<u8>>>,
        valid_keys: HashSet<u8>,
    }

    impl TestTimer {
        fn new(responses: Vec<Poll<Option<u8>>>) -> Self {
            Self {
                responses,
                valid_keys: HashSet::new(),
            }
        }
    }

    impl KeyedTimer<u8> for TestTimer {
        fn clear(&mut self) {
            self.valid_keys.clear();
        }

        fn insert(&mut self, item_key: u8) {
            self.valid_keys.insert(item_key);
        }

        fn poll_expired(&mut self, _cx: &mut Context) -> Poll<Option<u8>> {
            match self.responses.pop() {
                Some(Poll::Pending) => unreachable!(),
                None | Some(Poll::Ready(None)) => Poll::Ready(None),
                Some(Poll::Ready(Some(k))) => {
                    if self.valid_keys.contains(&k) {
                        Poll::Ready(Some(k))
                    } else {
                        Poll::Ready(None)
                    }
                }
            }
        }
    }

    fn arb_timer() -> impl Strategy<Value = TestTimer> {
        // The timer always returns a `Poll::Ready` and never a `Poll::Pending`.
        Vec::<(bool, u8)>::arbitrary()
            .prop_map(|v| {
                v.into_iter()
                    .map(|(b, k)| {
                        if b {
                            Poll::Ready(Some(k))
                        } else {
                            Poll::Ready(None)
                        }
                    })
                    .collect()
            })
            .prop_map(TestTimer::new)
    }

    /// A test partitioner
    ///
    /// This partitioner is nothing special. It has a large-ish key space but
    /// not so large that we'll never see batches accumulate properly.
    #[pin_project]
    #[derive(Debug)]
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

            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let batcher = PartitionedBatcher::with_timer(&mut stream, partitioner, timer,
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
            let noop_waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&noop_waker);

            let mut stream = stream::iter(stream.into_iter());
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = PartitionedBatcher::with_timer(&mut stream, partitioner,
                                                  timer, item_limit,
                                                  Some(allocation_limit));
            let mut batcher = Pin::new(&mut batcher);

            loop {
                match batcher.as_mut().poll_next(&mut cx) {
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
            let noop_waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&noop_waker);

            let mut partitions = separate_partitions(stream.clone(), &partitioner);

            let mut stream = stream::iter(stream.into_iter());
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = PartitionedBatcher::with_timer(&mut stream, partitioner,
                                                  timer, item_limit,
                                                  Some(allocation_limit));
            let mut batcher = Pin::new(&mut batcher);

            loop {
                match batcher.as_mut().poll_next(&mut cx) {
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
            let noop_waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&noop_waker);

            let total_items = stream.len();
            let mut stream = stream::iter(stream.into_iter());
            let item_limit = NonZeroUsize::new(item_limit as usize).unwrap();
            let allocation_limit = NonZeroUsize::new(allocation_limit as usize).unwrap();
            let mut batcher = PartitionedBatcher::with_timer(&mut stream, partitioner,
                                                  timer, item_limit,
                                                  Some(allocation_limit));
            let mut batcher = Pin::new(&mut batcher);

            let mut observed_items = 0;
            loop {
                match batcher.as_mut().poll_next(&mut cx) {
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

    #[tokio::test(start_paused = true)]
    #[allow(clippy::semicolon_if_nothing_returned)] // https://github.com/rust-lang/rust-clippy/issues/7438
    async fn expiration_queue_impl_keyed_timer() {
        // Asserts that ExpirationQueue properly implements KeyedTimer. We are
        // primarily concerned with whether expiration is properly observed.
        let timeout = Duration::from_millis(100); // 1/10 of a second, an
                                                  // eternity

        let mut expiration_queue: ExpirationQueue<u8> = ExpirationQueue::new(timeout);

        // If the queue is empty assert that when we poll for expired entries
        // nothing comes back.
        assert_eq!(0, expiration_queue.len());
        let result = single_poll(|cx| expiration_queue.poll_expired(cx));
        assert_eq!(result, Poll::Ready(None));

        // Insert an item key into the queue. Assert that the size of the queue
        // has grown, assert that the queue still does not believe the item has expired, and _then_
        // advance time enough to allow it to expire, and assert that it has.
        expiration_queue.insert(128);
        assert_eq!(1, expiration_queue.len());

        let result = single_poll(|cx| expiration_queue.poll_expired(cx));
        assert_eq!(result, Poll::Pending);

        advance(timeout + Duration::from_nanos(1)).await;
        let result = single_poll(|cx| expiration_queue.poll_expired(cx));
        assert_eq!(result, Poll::Ready(Some(128)));
        let result = single_poll(|cx| expiration_queue.poll_expired(cx));
        assert_eq!(result, Poll::Ready(None));

        // Now we poll assured that the queue has emptied out again.
        assert_eq!(0, expiration_queue.len());
        let result = single_poll(|cx| expiration_queue.poll_expired(cx));
        assert_eq!(result, Poll::Ready(None));

        // Finally, blitz a handful of items into the queue, assert its size,
        // clear the queue and assert it as being empty.
        expiration_queue.insert(128);
        expiration_queue.insert(64);
        expiration_queue.insert(32);
        assert_eq!(3, expiration_queue.len());
        expiration_queue.clear();
        assert_eq!(0, expiration_queue.len());
        let result = single_poll(|cx| expiration_queue.poll_expired(cx));
        assert_eq!(result, Poll::Ready(None));
    }

    fn single_poll<T, F>(mut f: F) -> Poll<T>
    where
        F: FnMut(&mut Context<'_>) -> Poll<T>,
    {
        let noop_waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&noop_waker);

        f(&mut cx)
    }
}
