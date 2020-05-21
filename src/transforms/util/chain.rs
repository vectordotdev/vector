//! Chain transforms.

#![deny(missing_docs)]

use crate::{event::Event, transforms::Transform};

/// Chains two transforms, passing the event through the first one (`.0`), and
/// then, if there's still an event to pass, through the second one (`.1`).
pub struct Two<A: Transform, B: Transform>(pub A, pub B);

impl<A: Transform, B: Transform> Two<A, B> {
    /// Creates a new chain of two transforms.
    pub fn new(first: A, second: B) -> Self {
        Self(first, second)
    }
}

impl<A: Transform, B: Transform> Transform for Two<A, B> {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let event = self.0.transform(event)?;
        self.1.transform(event)
    }
}
