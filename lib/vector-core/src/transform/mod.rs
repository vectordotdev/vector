use crate::event::Event;
use futures::Stream;
use std::pin::Pin;
#[cfg(any(feature = "lua"))]
pub mod runtime_transform;
pub use config::{DataType, ExpandType, TransformConfig, TransformContext};

mod config;

/// Transforms come in two variants. Functions, or tasks.
///
/// While function transforms can be run out of order, or concurrently, task
/// transforms act as a coordination or barrier point.
pub enum Transform {
    Function(Box<dyn FunctionTransform>),
    FallibleFunction(Box<dyn FallibleFunctionTransform>),
    Task(Box<dyn TaskTransform>),
}

impl Transform {
    /// Create a new function transform.
    ///
    /// These functions are "stateless" and can be run in parallel, without
    /// regard for coordination.
    ///
    /// **Note:** You should prefer to implement this over [`TaskTransform`]
    /// where possible.
    pub fn function(v: impl FunctionTransform + 'static) -> Self {
        Transform::Function(Box::new(v))
    }

    /// Mutably borrow the inner transform as a function transform.
    ///
    /// # Panics
    ///
    /// If the transform is not a [`FunctionTransform`] this will panic.
    pub fn as_function(&mut self) -> &mut Box<dyn FunctionTransform> {
        match self {
            Transform::Function(t) => t,
            _ => panic!(
                "Called `Transform::as_function` on something that was not a function variant."
            ),
        }
    }

    /// Transmute the inner transform into a function transform.
    ///
    /// # Panics
    ///
    /// If the transform is not a [`FunctionTransform`] this will panic.
    pub fn into_function(self) -> Box<dyn FunctionTransform> {
        match self {
            Transform::Function(t) => t,
            _ => panic!(
                "Called `Transform::into_function` on something that was not a function variant."
            ),
        }
    }

    /// Create a new fallible function transform.
    ///
    /// There are similar to `FunctionTransform`, but with a second output for events that
    /// encountered an error during processing.
    pub fn fallible_function(v: impl FallibleFunctionTransform + 'static) -> Self {
        Transform::FallibleFunction(Box::new(v))
    }

    /// Mutably borrow the inner transform as a fallible function transform.
    ///
    /// # Panics
    ///
    /// If the transform is not a [`FallibleFunctionTransform`] this will panic.
    pub fn as_fallible_function(&mut self) -> &mut Box<dyn FallibleFunctionTransform> {
        match self {
            Transform::FallibleFunction(t) => t,
            _ => panic!(
                "Called `Transform::as_fallible_function` on something that was not a fallible function variant."
            ),
        }
    }

    /// Transmute the inner transform into a fallible function transform.
    ///
    /// # Panics
    ///
    /// If the transform is not a [`FallibleFunctionTransform`] this will panic.
    pub fn into_fallible_function(self) -> Box<dyn FallibleFunctionTransform> {
        match self {
            Transform::FallibleFunction(t) => t,
            _ => panic!(
                "Called `Transform::into_fallible_function` on something that was not a fallible function variant."
            ),
        }
    }

    /// Create a new task transform.
    ///
    /// These tasks are coordinated, and map a stream of some `U` to some other
    /// `T`.
    ///
    /// **Note:** You should prefer to implement [`FunctionTransform`] over this
    /// where possible.
    pub fn task(v: impl TaskTransform + 'static) -> Self {
        Transform::Task(Box::new(v))
    }

    /// Mutably borrow the inner transform as a task transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`FunctionTransform`] this will panic.
    pub fn as_task(&mut self) -> &mut Box<dyn TaskTransform> {
        match self {
            Transform::Task(t) => t,
            _ => {
                panic!("Called `Transform::as_task` on something that was not a task variant.")
            }
        }
    }

    /// Transmute the inner transform into a task transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`FunctionTransform`] this will panic.
    pub fn into_task(self) -> Box<dyn TaskTransform> {
        match self {
            Transform::Task(t) => t,
            _ => {
                panic!("Called `Transform::into_task` on something that was not a task variant.")
            }
        }
    }
}

/// Transforms that are simple, and don't require attention to coordination.
/// You can run them as simple functions over events in any order.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a
///   `TaskTransform` or vice versa.
pub trait FunctionTransform: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event);
}

dyn_clone::clone_trait_object!(FunctionTransform);

/// Similar to `FunctionTransform`, but with a second output for events that encountered an error
/// during processing.
pub trait FallibleFunctionTransform: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, output: &mut Vec<Event>, errors: &mut Vec<Event>, event: Event);
}

dyn_clone::clone_trait_object!(FallibleFunctionTransform);

// For testing, it's convenient to ignore the error output and continue to use helpers like
// `transform_one`.
impl<T> FunctionTransform for T
where
    T: FallibleFunctionTransform,
{
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let mut err_buf = Vec::new();
        self.transform(output, &mut err_buf, event)
    }
}

/// Transforms that tend to be more complicated runtime style components.
///
/// These require coordination and map a stream of some `T` to some `U`.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a
/// `TaskTransform` or vice versa.
pub trait TaskTransform: Send {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static;
}
