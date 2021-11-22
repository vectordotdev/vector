use super::data;
use super::limiter;
use core_common::byte_size_of::ByteSizeOf;
use std::time::Duration;

use crate::stream::batcher::data::BatchReduce;
use crate::stream::batcher::limiter::{ByteSizeOfItemSize, ItemBatchSize, SizeLimit};
use crate::stream::BatcherSettings;
use data::BatchData;
use limiter::BatchLimiter;

pub struct BatchConfigParts<L, D> {
    batch_limiter: L,
    batch_data: D,
    timeout: Duration,
}

pub trait BatchConfig<T> {
    type ItemMetadata;
    type Batch;

    fn batch_len(&self) -> usize;
    fn take_batch(&mut self) -> Self::Batch;
    fn push_item(&mut self, item: T, metadata: Self::ItemMetadata);

    fn is_batch_full(&self) -> bool;
    fn item_fits_in_batch(&self, item: &T) -> (bool, Self::ItemMetadata);

    fn timeout(&self) -> Duration;
}

impl<T, L, B> BatchConfig<T> for BatchConfigParts<L, B>
where
    L: BatchLimiter<T, B>,
    B: BatchData<T>,
{
    type ItemMetadata = L::ItemMetadata;
    type Batch = B::Batch;

    fn batch_len(&self) -> usize {
        self.batch_data.len()
    }

    fn take_batch(&mut self) -> Self::Batch {
        self.batch_limiter.take_batch();
        self.batch_data.take_batch()
    }

    fn push_item(&mut self, item: T, metadata: Self::ItemMetadata) {
        self.batch_data.push_item(item);
        self.batch_limiter.push_item(metadata);
    }

    fn is_batch_full(&self) -> bool {
        self.batch_limiter.is_batch_full(&self.batch_data)
    }

    fn item_fits_in_batch(&self, item: &T) -> (bool, Self::ItemMetadata) {
        self.batch_limiter
            .item_fits_in_batch(item, &self.batch_data)
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// A batcher config using the `ItemBatchSize<T>` trait to determine batch sizes.
/// The output type is generic.
pub fn item_size<B, I, T>(
    settings: BatcherSettings,
    batch: B,
    item_size: I,
) -> BatchConfigParts<SizeLimit<I>, B>
where
    B: BatchData<T>,
    I: ItemBatchSize<T>,
{
    BatchConfigParts {
        batch_limiter: SizeLimit {
            batch_size_limit: settings.size_limit,
            batch_item_limit: settings.item_limit,
            current_size: 0,
            item_size_calculator: item_size,
        },
        batch_data: batch,
        timeout: settings.timeout,
    }
}

/// A batcher config using the `ByteSizeOf` trait to determine batch sizes.
/// The output type is generic.
pub fn byte_size_of<B, T>(
    settings: BatcherSettings,
    batch: B,
) -> BatchConfigParts<SizeLimit<ByteSizeOfItemSize>, B>
where
    B: BatchData<T>,
    T: ByteSizeOf,
{
    item_size(settings, batch, ByteSizeOfItemSize)
}

/// A batcher config using the `ByteSizeOf` trait to determine batch sizes.
/// The output type is generic.
pub fn byte_size_of_vec<T>(
    settings: BatcherSettings,
) -> BatchConfigParts<SizeLimit<ByteSizeOfItemSize>, Vec<T>>
where
    T: ByteSizeOf,
{
    item_size(settings, vec![], ByteSizeOfItemSize)
}

/// A batcher config using the `ItemBatchSize` trait to determine batch sizes.
/// The output is built with the supplied reducer function.
pub fn item_size_reducer<I, T, F, S>(
    settings: BatcherSettings,
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
            batch_size_limit: settings.size_limit,
            batch_item_limit: settings.item_limit,
            current_size: 0,
            item_size_calculator: item_size,
        },
        batch_data: BatchReduce::new(reducer),
        timeout: settings.timeout,
    }
}
