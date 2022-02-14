use std::hash::Hash;

/// Calculate partitions for an item
///
/// This trait allows us to express in the type system that for some `Item` we
/// are able to calculate a `Key` that identifies that item.
pub trait Partitioner {
    type Item;
    type Key: Clone + Eq + Hash;

    /// Partition the `Item` by calculating its `Key`
    ///
    /// The resulting key should ideally be unique for an `Item` or arrived at
    /// in such a way that if two distinct `Item` instances partition to the
    /// same key they are mergeable if put into the same collection by this key.
    fn partition(&self, item: &Self::Item) -> Self::Key;
}
