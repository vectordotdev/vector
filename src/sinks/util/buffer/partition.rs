use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    task::Poll,
};

use futures::{poll, StreamExt};
use tokio_util::time::{delay_queue::Key, DelayQueue};
use vector_core::{ByteSizeOf, event::{EventFinalizers, Finalizable}, partition::Partitioner};

use crate::sinks::util::batch::{Batch, BatchConfig, BatchError, BatchSettings, PushResult};
use crate::sinks::util::BatchSize;

pub trait Partition<K> {
    fn partition(&self) -> K;
}
#[derive(Debug)]
pub struct PartitionBuffer<T, K> {
    inner: T,
    key: Option<K>,
}

#[derive(Debug, Clone)]
pub struct PartitionInnerBuffer<T, K> {
    pub(self) inner: T,
    key: K,
}

impl<T, K> PartitionBuffer<T, K> {
    pub fn new(inner: T) -> Self {
        Self { inner, key: None }
    }
}

impl<T, K> Batch for PartitionBuffer<T, K>
where
    T: Batch,
    K: Clone,
{
    type Input = PartitionInnerBuffer<T::Input, K>;
    type Output = PartitionInnerBuffer<T::Output, K>;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(T::get_settings_defaults(config, defaults.into())?.into())
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let key = item.key;
        match self.inner.push(item.inner) {
            PushResult::Ok(full) => {
                self.key = Some(key);
                PushResult::Ok(full)
            }
            PushResult::Overflow(inner) => PushResult::Overflow(Self::Input { inner, key }),
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(self.inner.fresh())
    }

    fn finish(mut self) -> Self::Output {
        let key = self.key.take().unwrap();
        let inner = self.inner.finish();
        PartitionInnerBuffer { inner, key }
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

impl<T, K> PartitionInnerBuffer<T, K> {
    pub fn new(inner: T, key: K) -> Self {
        Self { inner, key }
    }

    pub fn into_parts(self) -> (T, K) {
        (self.inner, self.key)
    }
}

impl<T, K> Partition<K> for PartitionInnerBuffer<T, K>
where
    K: Clone,
{
    fn partition(&self) -> K {
        self.key.clone()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchPushResult<I> {
    /// The item was partitioned and batched successfully.
    ///
    /// The boolean value indicates whether or not the batch is now full after pushing the item in.
    Success(bool),

    /// The batch was already full.
    Overflow(I),

    /// The batcher indicated that it is closed and is no longer taking items.
    Closed(I),

    /// The item failed to be pushed into the batch due to an error during partitioning.
    Failure(I),
}

impl<I> BatchPushResult<I> {
    /// Whether or not the given batch should be flushed.
    pub fn should_flush(&self) -> bool {
        match self {
            BatchPushResult::Success(full) => *full,
            BatchPushResult::Overflow(_) => true,
            BatchPushResult::Closed(_) => true,
            _ => false,
        }
    }

    /// Takes the inner object if one exists.
    pub fn into_inner(self) -> Option<I> {
        match self {
            BatchPushResult::Overflow(item) => Some(item),
            BatchPushResult::Closed(item) => Some(item),
            BatchPushResult::Failure(item) => Some(item),
            _ => None,
        }
    }
}

/// An in-progress batch for `PartitionBatch`.
///
/// Handles enforcing batch size limits (total size and total number of events) as well as
/// coalsescing event finalizers for the overall batch.
pub struct PartitionInFlightBatch<P>
where
    P: Partitioner,
{
    closed: bool,
    items: Vec<P::Item>,
    finalizers: EventFinalizers,
    total_size: usize,
    size: BatchSize<()>,
    delay_id: Option<Key>,
    _partitioner: PhantomData<P>,
}

impl<P> PartitionInFlightBatch<P>
where
    P: Partitioner,
    P::Item: ByteSizeOf + Finalizable,
{
    pub fn new(size: BatchSize<()>) -> Self {
        trace!(
            "new batch sizing: {} bytes or {} items",
            size.bytes,
            size.events
        );
        Self {
            closed: false,
            items: Vec::new(),
            finalizers: EventFinalizers::default(),
            total_size: 0,
            size,
            delay_id: None,
            _partitioner: PhantomData,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.closed || self.items.len() == self.size.events || self.total_size >= self.size.bytes
    }

    pub fn set_delay_id(&mut self, id: Key) {
        self.delay_id = Some(id);
    }

    pub fn delay_id(&self) -> Option<&Key> {
        self.delay_id.as_ref()
    }

    pub fn push(&mut self, mut item: P::Item) -> BatchPushResult<P::Item> {
        // Don't overrun our batch size in bytes.
        let item_size = item.allocated_bytes();
        if self.total_size + item_size > self.size.bytes {
            // This is a corner case, but one that isn't actually particularly rare:
            //
            // If we have a batch that, for example, is using 99999 bytes out of 100000 bytes, the next
            // push attempt with an item that's 2 bytes or bigger will fail.  Obviously, going over our limit
            // in small increments isn't truly an issue: our memory limits are best-effort, not in
            // the same vein as a hard real-time system.
            //
            // If we reject the item but leave the batch as-is, technically, the batch is not yet
            // full: we haven't exceeded the size limit, or item count limit.  This poses a problem
            // for downstream usage, where callers will likely temporarily hold the item and try to
            // flush the batch so that they can attempt to push it again. However, therein lies the
            // problem: our batch doesn't actually believe it's full yet, so we're in a conundrum.
            //
            // Instead, we mark the batch closed here so that the next caller to inspect it is
            // notified that it is full and should be flushed.  This is splitting the difference:
            // it's not yet full, we haven't overflowed it, but it clearly needs to be cleared to
            // allow progress by the caller.
            //
            // We don't need to do this in the item count path, because that one only happens in
            // discrete increments of 1; we can either add another item or we cannot.
            self.closed = true;
            return BatchPushResult::Overflow(item);
        }

        // Don't overrun our batch size in events.
        let item_limit = self.size.events;
        let current_items = self.items.len();
        if current_items == item_limit {
            return BatchPushResult::Overflow(item);
        }

        // Add the item to the batch, and do the necessary accounting.
        let finalizers = item.take_finalizers();
        self.items.push(item);
        self.finalizers.merge(finalizers);
        self.total_size += item_size;

        BatchPushResult::Success(self.is_full())
    }

    pub fn finish(self, key: P::Key) -> PartitionFinishedBatch<P> {
        trace!(
            "batch finished with {} bytes, {} items",
            self.total_size,
            self.items.len()
        );
        PartitionFinishedBatch {
            key,
            items: self.items,
            total_size: self.total_size,
            finalizers: self.finalizers,
            _partitioner: PhantomData,
        }
    }
}

/// A complete partition batch.
#[derive(Clone)]
pub struct PartitionFinishedBatch<P>
where
    P: Partitioner,
{
    key: P::Key,
    items: Vec<P::Item>,
    total_size: usize,
    finalizers: EventFinalizers,
    _partitioner: PhantomData<P>,
}

impl<P> PartitionFinishedBatch<P>
where
    P: Partitioner,
{
    pub fn key(&self) -> &P::Key {
        &self.key
    }

    pub fn items(&self) -> &[P::Item] {
        self.items.as_slice()
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn into_parts(self) -> (P::Key, Vec<P::Item>, EventFinalizers) {
        (self.key, self.items, self.finalizers)
    }
}

/// Batteries-included partitioning batcher.
///
/// Provides simple batching of events based on user-defined partitioning.  In addition, batching
/// can be coinfigured in both time and space.  Finalization of events is provided as a first-class citizen.
pub struct PartitionBatcher<P>
where
    P: Partitioner,
    P::Item: ByteSizeOf + Finalizable,
{
    partitioner: P,
    settings: BatchSettings<()>,
    timeout_queue: DelayQueue<P::Key>,
    batches: HashMap<P::Key, PartitionInFlightBatch<P>>,
    closed: bool,
}

impl<P> PartitionBatcher<P>
where
    P: Partitioner,
    P::Item: ByteSizeOf + Finalizable,
{
    /// Creates a new `PartitionBatcher`.
    pub fn new(partitioner: P, settings: BatchSettings<()>) -> Self {
        PartitionBatcher {
            partitioner,
            settings,
            timeout_queue: DelayQueue::new(),
            batches: HashMap::new(),
            closed: false,
        }
    }

    /// Marks this batcher as closed.
    ///
    /// All future calls to `get_ready_batches` will return all in-flight batches regardless of
    /// whether or not they're full and whether or not they've timed out.  This allows callers to
    /// retrieve all in-flight batches in the case of Vector shutting down.
    ///
    /// This will also remove all pending batch expirations, so the caller must call
    /// `get_ready_batches` at least once after calling `close` to ensure all remaining batches are
    /// retrieved.
    pub fn close(&mut self) {
        self.closed = true;
        self.timeout_queue.clear();
    }

    /// Pushes an item into its corresponding batch.
    ///
    /// If there was an item
    pub fn push(&mut self, item: P::Item) -> BatchPushResult<P::Item> {
        if self.closed {
            return BatchPushResult::Closed(item);
        }

        match self.partitioner.partition(&item) {
            Some(pk) => {
                // TODO: any good way to push this clone into the closure for or_insert_with without
                // stacked borrows? or another general approach that defers the clone?
                let size = self.settings.size.clone();
                let mut new_batch_pk = None;
                let batch = self.batches.entry(pk.clone()).or_insert_with_key(|k| {
                    new_batch_pk = Some(k.clone());
                    PartitionInFlightBatch::new(size)
                });

                // If we've created a new batch, we need to shove it into our timeout queue.
                if let Some(pk) = new_batch_pk {
                    // Don't register this batch for expiration unless the timeout is actually
                    // greater than zero, as a zero timeout is our "don't ever timeout" sentinel
                    // value.
                    //
                    // TODO: Should probably be an Option<Duration>... hmmm...
                    if self.settings.timeout.as_nanos() != 0 {
                        let id = self.timeout_queue.insert(pk, self.settings.timeout);
                        batch.set_delay_id(id);
                    }
                }

                batch.push(item)
            }
            None => BatchPushResult::Failure(item),
        }
    }

    pub async fn get_ready_batches(&mut self) -> Option<Vec<PartitionFinishedBatch<P>>> {
        let mut batches = Vec::new();

        // Check to see if any batches are full and need to be flushed out.
        let mut ready_partitions = self
            .batches
            .iter()
            .filter_map(|(pk, b)| {
                if b.is_full() || self.closed {
                    Some(pk.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();

        // Check to see if any batches have expired, indicating a need for them to be flushed.  We
        // explicitly use the `poll!` macro to poll the delay queue, which holds all batch
        // expirations.  We do this so that we don't actually wait until the next batch has expired,
        // which might block the task from accepting more items.  However, this differs from
        // `FutureExt::now_and_never` in that `poll!` ensures this task context is properly attached
        // so that the next batch expiration wakes us up.
        //
        // We gate the polling of timed out batches to ensure we don't end up with duplicates when
        // the batcher flips to closed mode.
        let mut requires_removal = ready_partitions.clone();
        if !self.closed {
            while let Poll::Ready(Some(Ok(pk))) = poll!(self.timeout_queue.next()) {
                let pk = pk.into_inner();
                let _ = requires_removal.remove(&pk);
                let _ = ready_partitions.insert(pk);
            }
        }

        for pk in ready_partitions {
            let batch = self.batches.remove(&pk).expect("batch must always exist");

            // Make sure we only try to remove the timeout entry when we didn't see it during this
            // iteration, since removal from DelayQueue will panic if the item doesn't exist.
            if let Some(delay_id) = batch.delay_id() {
                if !self.closed && requires_removal.contains(&pk) {
                    let _ = self.timeout_queue.remove(delay_id);
                }
            }

            batches.push(batch.finish(pk));
        }

        if !batches.is_empty() {
            Some(batches)
        } else {
            None
        }
    }
}
