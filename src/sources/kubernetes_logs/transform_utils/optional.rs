//! Optional transform.

#![deny(missing_docs)]

use crate::{event::Event, transforms::FunctionTransform};

/// Optional transform.
/// Passes events through the specified transform is any, otherwise passes them,
/// as-is.
/// Useful to avoid boxing the transforms.
pub struct Optional<T: FunctionTransform>(pub Option<T>);

impl<T: FunctionTransform> FunctionTransform for Optional<T> {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        match self.0 {
            Some(ref mut val) => val.transform(event),
            None => Some(event),
        }
    }
}
