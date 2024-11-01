use std::time::Duration;

use data::BatchData;
use limiter::BatchLimiter;

use super::{data, limiter};

pub struct BatchConfigParts<L, D> {
    pub batch_limiter: L,
    pub batch_data: D,
    pub timeout: Duration,
}

pub trait BatchConfig<T> {
    type ItemMetadata;
    type Batch;

    /// Returns the number of elements in the batch
    fn len(&self) -> usize;

    /// Determines whether the batch is empty or not
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the current batch, and resets any internal state
    fn take_batch(&mut self) -> Self::Batch;

    /// Adds a single item to the batch, with the given metadata that was calculated by `item_fits_in_batch`
    fn push(&mut self, item: T, metadata: Self::ItemMetadata);

    /// Returns true if it is not possible for another item to fit in the batch
    fn is_batch_full(&self) -> bool;

    /// It is safe to assume that `is_batch_full` would return `false` before this is called.
    /// You can return arbitrary metadata for an item that will be given back when the item
    /// is actually pushed onto the batch. This is useful if there is an expensive calculation
    /// to determine the "size" of the item.
    fn item_fits_in_batch(&self, item: &T) -> (bool, Self::ItemMetadata);

    /// Returns the maximum amount of time to wait for inputs to a single batch.
    /// The timer starts when the first item is received for a batch.
    fn timeout(&self) -> Duration;
}

impl<T, L, B> BatchConfig<T> for BatchConfigParts<L, B>
where
    L: BatchLimiter<T, B>,
    B: BatchData<T>,
{
    type ItemMetadata = L::ItemMetadata;
    type Batch = B::Batch;

    fn len(&self) -> usize {
        self.batch_data.len()
    }

    fn take_batch(&mut self) -> Self::Batch {
        self.batch_limiter.reset();
        self.batch_data.take_batch()
    }

    fn push(&mut self, item: T, metadata: Self::ItemMetadata) {
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
