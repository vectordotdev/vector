//! Optional transform.

#![deny(missing_docs)]

use std::pin::Pin;

use futures::Stream;

use crate::{event::Event, transforms::TaskTransform};

/// Optional transform.
/// Passes events through the specified transform is any, otherwise passes them,
/// as-is.
/// Useful to avoid boxing the transforms.
#[derive(Clone, Debug)]
pub struct Optional<T: TaskTransform>(pub Option<T>);

impl<T: TaskTransform> TaskTransform for Optional<T> {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        match self.0 {
            Some(val) => Box::new(val).transform(task),
            None => task,
        }
    }
}
