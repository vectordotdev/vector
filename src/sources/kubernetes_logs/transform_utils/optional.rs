//! Optional transform.

#![deny(missing_docs)]

use crate::{event::Event, transforms::TaskTransform};
use futures01::Stream as Stream01;

/// Optional transform.
/// Passes events through the specified transform is any, otherwise passes them,
/// as-is.
/// Useful to avoid boxing the transforms.
#[derive(Clone, Debug)]
pub struct Optional<T: TaskTransform>(pub Option<T>);

impl<T: TaskTransform> TaskTransform for Optional<T> {
    fn transform(
        self: Box<Self>,
        task: Box<dyn Stream01<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn Stream01<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        match self.0 {
            Some(val) => Box::new(val).transform(task),
            None => task,
        }
    }
}
