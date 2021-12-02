use crate::{
    event::Event,
    fanout::{self, Fanout},
    ByteSizeOf,
};
use futures::SinkExt;
use futures::Stream;
use std::{collections::HashMap, pin::Pin};

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
        self.transform(output, &mut err_buf, event);
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

/// This currently a topology-focused trait that unifies function and fallible function transforms.
/// Eventually it (or something very similar) should be able to replace both entirely. That will
/// likely involve it not being batch-focused anymore, and since we'll then be able to have
/// a single implementation of these loops that apply across all sync transforms.
pub trait SyncTransform: Send + Sync {
    fn run(&mut self, events: Vec<Event>, outputs: &mut TransformOutputs);
}

impl SyncTransform for Box<dyn FallibleFunctionTransform> {
    fn run(&mut self, events: Vec<Event>, outputs: &mut TransformOutputs) {
        let mut buf = Vec::with_capacity(1);
        let mut err_buf = Vec::with_capacity(1);

        for v in events {
            self.transform(&mut buf, &mut err_buf, v);
            outputs.append(&mut buf);
            // TODO: this is a regession in the number of places that we hardcode this name, but it
            // is temporary because we're quite close to being able to remove the overly-specific
            // `FallibleFunctionTransform` trait entirely.
            outputs.append_named("dropped", &mut err_buf);
        }
    }
}

impl SyncTransform for Box<dyn FunctionTransform> {
    fn run(&mut self, events: Vec<Event>, outputs: &mut TransformOutputs) {
        let mut buf = Vec::with_capacity(4); // also an arbitrary,
                                             // smallish constant
        for v in events {
            self.transform(&mut buf, v);
            outputs.append(&mut buf);
        }
    }
}

/// This struct manages collecting and forwarding the various outputs of transforms. It's designed
/// to unify the interface for transforms that may or may not have more than one possible output
/// path. It's currently batch-focused for use in topology-level tasks, but can easily be extended
/// to be used directly by transforms via a new, simpler trait interface.
pub struct TransformOutputs {
    primary_buffer: Vec<Event>,
    named_buffers: HashMap<String, Vec<Event>>,
    primary_output: Fanout,
    named_outputs: HashMap<String, Fanout>,
}

impl TransformOutputs {
    pub fn new_with_capacity(
        named_outputs_in: Vec<String>,
        capacity: usize,
    ) -> (Self, HashMap<Option<String>, fanout::ControlChannel>) {
        let mut named_buffers = HashMap::new();
        let mut named_outputs = HashMap::new();
        let mut controls = HashMap::new();

        for name in named_outputs_in {
            let (fanout, control) = Fanout::new();
            named_outputs.insert(name.clone(), fanout);
            controls.insert(Some(name.clone()), control);
            named_buffers.insert(name.clone(), Vec::new());
        }

        let (primary_output, control) = Fanout::new();
        let me = Self {
            primary_buffer: Vec::with_capacity(capacity),
            named_buffers,
            primary_output,
            named_outputs,
        };
        controls.insert(None, control);

        (me, controls)
    }

    pub fn append(&mut self, slice: &mut Vec<Event>) {
        self.primary_buffer.append(slice);
    }

    pub fn append_named(&mut self, name: &str, slice: &mut Vec<Event>) {
        self.named_buffers
            .get_mut(name)
            .expect("unknown output")
            .append(slice);
    }

    pub fn len(&self) -> usize {
        self.primary_buffer.len()
            + self
                .named_buffers
                .iter()
                .map(|(_, buf)| buf.len())
                .sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub async fn flush(&mut self) {
        flush_inner(&mut self.primary_buffer, &mut self.primary_output).await;
        for (key, buf) in &mut self.named_buffers {
            flush_inner(
                buf,
                self.named_outputs.get_mut(key).expect("unknown output"),
            )
            .await;
        }
    }
}

async fn flush_inner(buf: &mut Vec<Event>, output: &mut Fanout) {
    for event in buf.drain(..) {
        output.feed(event).await.expect("unit error");
    }
}

impl ByteSizeOf for TransformOutputs {
    fn allocated_bytes(&self) -> usize {
        self.primary_buffer.size_of()
            + self
                .named_buffers
                .iter()
                .map(|(_, buf)| buf.size_of())
                .sum::<usize>()
    }
}
