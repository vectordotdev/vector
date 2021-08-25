use std::{
    collections::HashMap, hash::Hash, io::Write, marker::PhantomData, task::Poll, time::Duration,
};

use bytes::Bytes;
use flate2::write::GzEncoder;
use futures::{poll, StreamExt};
use serde::Serialize;
use tokio_util::time::DelayQueue;
use vector_core::{
    event::{Event, EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::{
    batch::{FinalizersBatch, StatefulBatch},
    encoding::EncodingConfig,
    BatchSize, EncodedEvent,
};

use super::{
    super::batch::{Batch, BatchConfig, BatchError, BatchSettings, PushResult},
    Compression, GZIP_FAST,
};

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

/// Strategy for partitioning events.
pub trait Partitioner {
    type Item: ByteSizeOf + Finalizable;
    type Key: Clone + Eq + Hash;

    fn partition(&self, item: &Self::Item) -> Option<Self::Key>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchPushResult<I> {
    /// The item was partitioned and batched successfully.
    ///
    /// The boolean value indicates whether or not the batch is now full after pushing the item in.
    Success(bool),

    /// The batch was either already full or there was
    Overflow(I),

    /// The item failed to be pushed into the batch due to an error during partitioning.
    Failure(I),
}

/// An in-progress batch for `PartitionBatch`.
///
/// Handles enforcing batch size limits (total size and total number of events) as well as
/// coalsescing event finalizers for the overall batch.
pub struct PartitionInFlightBatch<P>
where
    P: Partitioner,
{
    items: Vec<P::Item>,
    finalizers: EventFinalizers,
    total_size: usize,
    size: BatchSize<()>,
    _partitioner: PhantomData<P>,
}

impl<P> PartitionInFlightBatch<P>
where
    P: Partitioner,
{
    pub fn new(size: BatchSize<()>) -> Self {
        Self {
            items: Vec::new(),
            finalizers: EventFinalizers::default(),
            total_size: 0,
            size,
            _partitioner: PhantomData,
        }
    }

    pub fn is_full(&self) -> bool {
        self.items.len() == self.size.events || self.total_size >= self.size.bytes
    }

    pub fn push(&mut self, item: P::Item) -> BatchPushResult<P::Item> {
        // Don't overrun our batch size in bytes.
        let item_size = item.allocated_bytes();
        if self.total_size + item_size > self.size.bytes {
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
{
    partitioner: P,
    settings: BatchSettings<()>,
    timeout_queue: DelayQueue<P::Key>,
    batches: HashMap<P::Key, PartitionInFlightBatch<P>>,
}

impl<P> PartitionBatcher<P>
where
    P: Partitioner,
{
    pub fn new(partitioner: P, settings: BatchSettings<()>) -> Self {
        PartitionBatcher {
            partitioner,
            settings,
            timeout_queue: DelayQueue::new(),
            batches: HashMap::new(),
        }
    }

    pub fn push(&mut self, item: P::Item) -> BatchPushResult<P::Item> {
        match self.partitioner.partition(&item) {
            Some(pk) => {
                let mut batch = self
                    .batches
                    .entry(pk)
                    .or_insert_with(|| PartitionInFlightBatch::new(self.settings.size.clone()));
                batch.push(item)
            }
            None => BatchPushResult::Failure(item),
        }
    }

    pub async fn get_ready_batches(&mut self) -> Vec<PartitionFinishedBatch<P>> {
        let mut batches = Vec::new();

        // Check to see if any batches are full and need to be flushed out.
        let mut ready_partitions = self
            .batches
            .iter()
            .filter_map(|(pk, b)| if b.is_full() { Some(pk.clone()) } else { None })
            .collect::<Vec<_>>();

        // Check to see if any batches have expired, indicating a need for them to be flushed.  We
        // explicitly use the `poll!` macro to poll the delay queue, which holds all batch
        // expirations.  We do this so that we don't actually wait until the next batch has expired,
        // which might block the task from accepting more items.  However, this differs from
        // `FutureExt::now_and_never` in that `poll!` ensures this task context is properly attached
        // so that the next batch expiration wakes us up.
        while let Poll::Ready(Some(Ok(pk))) = poll!(self.timeout_queue.next()) {
            let pk = pk.into_inner();
            ready_partitions.push(pk);
        }

        for pk in ready_partitions {
            let batch = self.batches.remove(&pk).expect("batch must always exist");
            batches.push(batch.finish(pk));
        }

        batches
    }
}
