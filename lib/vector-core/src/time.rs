//! Time utilities for vector-core

use std::task::{Context, Poll};

/// A trait for representing many timer by key
///
/// Embedding time as a type into other types eases property testing and
/// verification. As such, this simple time type encodes the notion of elapsed
/// timing with reset. Multiple timers are tracked by key -- `K` -- and emitted
/// as they elapse.
pub trait KeyedTimer<K> {
    /// Clear the KeyedTimer
    ///
    /// This function clears all keys from the timer. This function will be
    /// empty afterward and if immediately called `poll_elapsed` will return
    /// `Poll::Ready(None)`.
    fn clear(&mut self);

    /// Insert a `K` into the KeyedTimer
    ///
    /// This function adds a new key into the timer. If the key previously
    /// existed for the same key the underlying key-timer is reset.
    fn insert(&mut self, item_key: K);

    // For an example of how property testing can use this type see the
    // `stream::Batcher` property tests.
    /// Whether a key-timer has elapsed or not.
    ///
    /// This function will return `Poll::Ready(None)` if a key-timer has not yet
    /// fired (or if the timer is empty), `Poll::Ready(Some(K))` if a key-timer
    /// has.
    fn poll_expired(&mut self, cx: &mut Context) -> Poll<Option<K>>;
}
