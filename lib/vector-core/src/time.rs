//! Time utilities for vector-core

use std::task::{Context, Poll};

/// A trait for representing timers
///
/// Embedding time as a type into other types eases property testing and
/// verification. As such, this simple time type encodes the notion of elapsed
/// timing with reset.
pub trait Timer {
    // For an example of how property testing can use this type see the
    // `stream::Batcher` property tests.
    /// Whether the timer has elapsed or not.
    ///
    /// This function will return `Poll::Pending` if the timer has not yet
    /// fired, `Poll::Ready(())` if it has.
    fn poll_elapsed(&mut self, cx: &mut Context) -> Poll<()>;
}
