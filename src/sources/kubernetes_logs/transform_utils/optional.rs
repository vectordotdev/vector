//! Optional transform.

#![deny(missing_docs)]

use std::pin::Pin;

use futures::Stream;

use crate::event::EventContainer;
use crate::transforms::TaskTransform;

/// Optional transform.
/// Passes events through the specified transform is any, otherwise passes them,
/// as-is.
/// Useful to avoid boxing the transforms.
#[derive(Clone, Debug)]
pub struct Optional<T>(pub Option<T>);

impl<T: TaskTransform<E>, E: EventContainer + 'static> TaskTransform<E> for Optional<T> {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = E> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = E> + Send>>
    where
        Self: 'static,
    {
        match self.0 {
            Some(val) => Box::new(val).transform(task),
            None => task,
        }
    }
}
