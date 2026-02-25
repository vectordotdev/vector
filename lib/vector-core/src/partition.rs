use std::{
    fmt::{self, Debug, Display},
    hash::Hash,
};

use crate::event::{EventFinalizers, EventStatus, Finalizable};

/// A wrapper for partition errors that enforces proper finalizer handling.
///
/// The inner error can only be accessed through methods that require
/// updating event finalizers, preventing silent error drops.
pub struct PartitionError<E> {
    inner: E,
}

impl<E> PartitionError<E> {
    /// Creates a new `PartitionError` wrapping the given error.
    pub fn new(error: E) -> Self {
        Self { inner: error }
    }

    /// Handle the error by updating finalizers and returning the inner error.
    pub fn handle(self, finalizers: &EventFinalizers) -> E {
        finalizers.update_status(EventStatus::Errored);
        self.inner
    }

    /// Handle the error by taking finalizers from a finalizable item and
    /// returning the inner error.
    pub fn handle_from<I: Finalizable>(self, item: &mut I) -> E {
        let finalizers = item.take_finalizers();
        self.handle(&finalizers)
    }

    /// Get a reference to the inner error for logging/metrics.
    pub fn error(&self) -> &E {
        &self.inner
    }

    /// Map the inner error to a different type.
    pub fn map<F, U>(self, f: F) -> PartitionError<U>
    where
        F: FnOnce(E) -> U,
    {
        PartitionError {
            inner: f(self.inner),
        }
    }
}

impl<E: Debug> Debug for PartitionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.inner, f)
    }
}

impl<E: Display> Display for PartitionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl<E: std::error::Error + 'static> std::error::Error for PartitionError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
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
    /// Returns a `PartitionError` if the key cannot be computed for the given item.
    /// The error wrapper ensures that finalizers are properly updated when handling
    /// partition failures.
    fn partition(&self, item: &Self::Item) -> Result<Self::Key, PartitionError<Self::Error>>;
}
