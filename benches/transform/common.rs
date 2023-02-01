//! A common suite of structs, functions et al that are useful for the
//! benchmarking of vector transforms.
use std::{
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{task::noop_waker, Stream};
use vector::event::{Event, LogEvent};

// == Streams ==

/// Consume a `Stream<T>` and do nothing with the received Items, runs to
/// completion
pub fn consume<T>(mut stream: Pin<Box<dyn Stream<Item = T>>>) {
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);

    while let Poll::Ready(Some(_)) = stream.as_mut().poll_next(&mut context) {}
}

// ==== FixedLogStream ====

/// A fixed size [`futures::stream::Stream`] of `Event::Log` instances.
#[derive(Debug, Clone)]
pub struct FixedLogStream {
    events: Vec<Event>,
}

impl FixedLogStream {
    /// Create a new `FixedLogStream` with `total` unspecified `Event` instances
    /// internal. `cycle_size` controls how often an `Event` will repeat.
    ///
    /// This constructor is useful for benchmarks where you do not care how the
    /// `Event`s are shaped, only that they exist.
    pub fn new(total: NonZeroUsize, cycle_size: NonZeroUsize) -> Self {
        let mut events = Vec::with_capacity(total.get());
        let mut cycle = 0;
        for _ in 0..total.get() {
            events.push(Event::Log(LogEvent::from(format!("event{}", cycle))));
            cycle = (cycle + 1) % cycle_size;
        }
        Self::new_from_vec(events)
    }

    /// Create a new `FixedLogStream` from an `Vec<Event>`
    ///
    /// This constructor is useful for benchmarks where you do care how the
    /// `Event`s are shaped, that is, their specific details are relevant to the
    /// measure you're trying to establish.
    pub fn new_from_vec(events: Vec<Event>) -> Self {
        FixedLogStream { events }
    }

    /// Return the length of the fixed stream
    ///
    /// This function will return the length of the items remaining in the
    /// stream.
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl Stream for FixedLogStream {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        Poll::Ready(this.events.pop())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.events.len(), Some(self.events.len()))
    }
}
