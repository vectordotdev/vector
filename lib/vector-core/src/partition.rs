use std::hash::Hash;

use crate::sink::PreparationError;

/// How an item should be routed before batching.
#[derive(Debug, PartialEq, Eq)]
pub enum PartitionDecision<K> {
    /// Batch the item under this key.
    Route(K),
    /// Intentionally discard the item, with a neutral acknowledgement.
    Discard,
}

/// Calculate partitions for an item
///
/// This trait allows us to express in the type system that for some `Item` we
/// are able to calculate a `Key` that identifies that item.
pub trait Partitioner {
    type Item;
    type Key: Clone + Eq + Hash;
    type Error: std::error::Error + Send;

    /// Partition the `Item` by calculating its `Key`
    ///
    /// The resulting key should ideally be unique for an `Item` or arrived at
    /// in such a way that if two distinct `Item` instances partition to the
    /// same key they are mergeable if put into the same collection by this key.
    ///
    /// # Errors
    ///
    /// Returns a preparation error if partitioning failed.
    fn partition(
        &self,
        item: &Self::Item,
    ) -> Result<PartitionDecision<Self::Key>, PreparationError<Self::Error>>;
}
