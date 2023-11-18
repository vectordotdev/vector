use vector_common::byte_size_of::ByteSizeOf;

use crate::batcher::data::BatchData;

pub trait BatchLimiter<T, B> {
    type ItemMetadata;

    /// Return true if it is not possible for another item to fit in the batch
    fn is_batch_full(&self, batch: &B) -> bool;

    /// It is safe to assume that `is_batch_full` would return `false` before this is called.
    /// You can return arbitrary metadata for an item that will be given back when the item
    /// is actually pushed onto the batch. This is useful if there is an expensive calculation
    /// to determine the "size" of the item.
    fn item_fits_in_batch(&self, item: &T, batch: &B) -> (bool, Self::ItemMetadata);

    /// Add a single item to the batch using the metadata that was calculated by `item_fits_in_batch`
    fn push_item(&mut self, metadata: Self::ItemMetadata);

    /// Reset internal state from a batch being taken.
    fn reset(&mut self);
}

pub struct SizeLimit<I> {
    /// The total "size" of all items in a batch. Size is intentionally
    /// vague here since it is user defined, and can vary.
    ///
    /// To ensure any individual event can be placed in a batch, the first element in a batch is not
    /// subject to this limit.
    pub batch_size_limit: usize,

    /// Total number of items that will be placed in a single batch.
    ///
    /// To ensure any individual event can be placed in a batch, the first element in a batch is not
    /// subject to this limit.
    pub batch_item_limit: usize,

    pub current_size: usize,
    pub item_size_calculator: I,
}

impl<T, B, I> BatchLimiter<T, B> for SizeLimit<I>
where
    B: BatchData<T>,
    I: ItemBatchSize<T>,
{
    type ItemMetadata = usize;

    fn is_batch_full(&self, batch: &B) -> bool {
        batch.len() >= self.batch_item_limit || self.current_size >= self.batch_size_limit
    }

    fn item_fits_in_batch(&self, item: &T, batch: &B) -> (bool, Self::ItemMetadata) {
        let item_size = self.item_size_calculator.size(item);
        if batch.len() == 0 {
            // make sure any individual item can always fit in a batch
            return (true, item_size);
        }
        let fits = self.current_size + item_size <= self.batch_size_limit;
        (fits, item_size)
    }

    fn push_item(&mut self, item_size: usize) {
        self.current_size += item_size;
    }

    fn reset(&mut self) {
        self.current_size = 0;
    }
}

pub trait ItemBatchSize<T> {
    /// The size of an individual item in a batch.
    fn size(&self, item: &T) -> usize;
}

pub struct ByteSizeOfItemSize;

impl<T: ByteSizeOf> ItemBatchSize<T> for ByteSizeOfItemSize {
    fn size(&self, item: &T) -> usize {
        item.size_of()
    }
}

impl<T, F> ItemBatchSize<T> for F
where
    F: Fn(&T) -> usize,
{
    fn size(&self, item: &T) -> usize {
        (self)(item)
    }
}
