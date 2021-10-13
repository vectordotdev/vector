use std::hash::Hash;
use std::marker::PhantomData;

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
    /// same key they are mergable if put into the same collection by this key.
    fn partition(&self, item: &Self::Item) -> Self::Key;
}

pub struct NullPartitioner<T> {
    item: PhantomData<T>,
}

impl<T> NullPartitioner<T> {
    pub fn new() -> NullPartitioner<T> {
        NullPartitioner { item: PhantomData }
    }
}

impl<T> Partitioner for NullPartitioner<T> {
    type Item = T;
    type Key = ();

    fn partition(&self, _item: &T) -> Self::Key {
        ()
    }
}
