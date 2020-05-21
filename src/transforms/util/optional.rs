//! Chain transforms.

#![deny(missing_docs)]

use crate::{event::Event, transforms::Transform};

/// Optional transform.
/// Passes events through the specified transform is any, otherwise passes them,
/// as-is.
/// Useful to avoid boxing the transforms.
pub struct Optional<T: Transform>(pub Option<T>);

impl<T: Transform> Transform for Optional<T> {
    fn transform(&mut self, event: Event) -> Option<Event> {
        match self.0 {
            Some(ref mut val) => val.transform(event),
            None => Some(event),
        }
    }
}
