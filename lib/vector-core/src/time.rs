//! Time utilities for vector-core

/// A trait for representing timers
///
/// Embedding time as a type into other types eases property testing and
/// verification. As such, this simple time type encodes the notion of elapsed
/// timing with reset.
pub trait Timer {
    // For an example of how property testing can use this type see the
    // `stream::Batcher` property tests.

    /// Whether the timer has elapsed or not, true if yes.
    fn has_elapsed(&self) -> bool;

    /// Signal that the timer ought to be reset.
    fn reset(&mut self);
}
