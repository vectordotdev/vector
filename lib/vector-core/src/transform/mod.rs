use std::{collections::HashMap, pin::Pin};

use futures::{stream, SinkExt, Stream, StreamExt};
use vector_common::internal_event::{emit, EventsSent, DEFAULT_OUTPUT};
use vector_common::EventDataEq;

use crate::{
    config::Output,
    event::{Event, EventArray, EventContainer},
    fanout::{self, Fanout},
    ByteSizeOf,
};

#[cfg(any(feature = "lua"))]
pub mod runtime_transform;
pub use config::{ExpandType, TransformConfig, TransformContext};

mod config;

/// Transforms come in two variants. Functions, or tasks.
///
/// While function transforms can be run out of order, or concurrently, task
/// transforms act as a coordination or barrier point.
pub enum Transform {
    Function(Box<dyn FunctionTransform>),
    Synchronous(Box<dyn SyncTransform>),
    Task(Box<dyn TaskTransform<EventArray>>),
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

    /// Create a new synchronous transform.
    ///
    /// This is a broader trait than the simple [`FunctionTransform`] in that it allows transforms
    /// to write to multiple outputs. Those outputs must be known in advanced and returned via
    /// `TransformConfig::outputs`. Attempting to send to any output not registered in advance is
    /// considered a bug and will cause a panic.
    pub fn synchronous(v: impl SyncTransform + 'static) -> Self {
        Transform::Synchronous(Box::new(v))
    }

    /// Create a new task transform.
    ///
    /// These tasks are coordinated, and map a stream of some `U` to some other
    /// `T`.
    ///
    /// **Note:** You should prefer to implement [`FunctionTransform`] over this
    /// where possible.
    pub fn task(v: impl TaskTransform<EventArray> + 'static) -> Self {
        Transform::Task(Box::new(v))
    }

    /// Create a new task transform over individual `Event`s.
    ///
    /// These tasks are coordinated, and map a stream of some `U` to some other
    /// `T`.
    ///
    /// **Note:** You should prefer to implement [`FunctionTransform`] over this
    /// where possible.
    ///
    /// # Panics
    ///
    /// TODO
    pub fn event_task(v: impl TaskTransform<Event> + 'static) -> Self {
        Transform::Task(Box::new(WrapEventTask(v)))
    }

    /// Mutably borrow the inner transform as a task transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`FunctionTransform`] this will panic.
    pub fn as_task(&mut self) -> &mut Box<dyn TaskTransform<EventArray>> {
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
    pub fn into_task(self) -> Box<dyn TaskTransform<EventArray>> {
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
    fn transform(&mut self, output: &mut OutputBuffer, event: Event);
}

dyn_clone::clone_trait_object!(FunctionTransform);

/// Transforms that tend to be more complicated runtime style components.
///
/// These require coordination and map a stream of some `T` to some `U`.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a
/// `TaskTransform` or vice versa.
pub trait TaskTransform<T: EventContainer + 'static>: Send + 'static {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>>;

    /// Wrap the transform task to process and emit individual
    /// events. This is used to simplify testing task transforms.
    fn transform_events(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        T: From<Event>,
        T::IntoIter: Send,
    {
        self.transform(task.map(Into::into).boxed())
            .map(EventContainer::into_events)
            .flat_map(stream::iter)
            .boxed()
    }
}

/// Broader than the simple [`FunctionTransform`], this trait allows transforms to write to
/// multiple outputs. Those outputs must be known in advanced and returned via
/// `TransformConfig::outputs`. Attempting to send to any output not registered in advance is
/// considered a bug and will cause a panic.
pub trait SyncTransform: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf);
}

dyn_clone::clone_trait_object!(SyncTransform);

impl<T> SyncTransform for T
where
    T: FunctionTransform,
{
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        FunctionTransform::transform(
            self,
            output.primary_buffer.as_mut().expect("no default output"),
            event,
        );
    }
}

// TODO: this is a bit ugly when we already have the above impl
impl SyncTransform for Box<dyn FunctionTransform> {
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        FunctionTransform::transform(
            self.as_mut(),
            output.primary_buffer.as_mut().expect("no default output"),
            event,
        );
    }
}

pub struct TransformOutputs {
    outputs_spec: Vec<Output>,
    primary_output: Option<Fanout>,
    named_outputs: HashMap<String, Fanout>,
}

impl TransformOutputs {
    pub fn new(outputs_in: Vec<Output>) -> (Self, HashMap<Option<String>, fanout::ControlChannel>) {
        let outputs_spec = outputs_in.clone();
        let mut primary_output = None;
        let mut named_outputs = HashMap::new();
        let mut controls = HashMap::new();

        for output in outputs_in {
            let (fanout, control) = Fanout::new();
            match output.port {
                None => {
                    primary_output = Some(fanout);
                    controls.insert(None, control);
                }
                Some(name) => {
                    named_outputs.insert(name.clone(), fanout);
                    controls.insert(Some(name.clone()), control);
                }
            }
        }

        let me = Self {
            outputs_spec,
            primary_output,
            named_outputs,
        };

        (me, controls)
    }

    pub fn new_buf_with_capacity(&self, capacity: usize) -> TransformOutputsBuf {
        TransformOutputsBuf::new_with_capacity(self.outputs_spec.clone(), capacity)
    }

    pub async fn send(&mut self, buf: &mut TransformOutputsBuf) {
        if let Some(primary) = self.primary_output.as_mut() {
            let count = buf.primary_buffer.as_ref().map_or(0, OutputBuffer::len);
            let byte_size = buf.primary_buffer.as_ref().map_or(0, ByteSizeOf::size_of);
            buf.primary_buffer
                .as_mut()
                .expect("mismatched outputs")
                .send(primary)
                .await;
            emit(&EventsSent {
                count,
                byte_size,
                output: Some(DEFAULT_OUTPUT),
            });
        }
        for (key, buf) in &mut buf.named_buffers {
            let count = buf.len();
            let byte_size = buf.size_of();
            buf.send(self.named_outputs.get_mut(key).expect("unknown output"))
                .await;
            emit(&EventsSent {
                count,
                byte_size,
                output: Some(key.as_ref()),
            });
        }
    }
}

pub struct TransformOutputsBuf {
    primary_buffer: Option<OutputBuffer>,
    named_buffers: HashMap<String, OutputBuffer>,
}

impl TransformOutputsBuf {
    pub fn new_with_capacity(outputs_in: Vec<Output>, capacity: usize) -> Self {
        let mut primary_buffer = None;
        let mut named_buffers = HashMap::new();

        for output in outputs_in {
            match output.port {
                None => {
                    primary_buffer = Some(OutputBuffer::with_capacity(capacity));
                }
                Some(name) => {
                    named_buffers.insert(name.clone(), OutputBuffer::default());
                }
            }
        }

        Self {
            primary_buffer,
            named_buffers,
        }
    }

    pub fn push(&mut self, event: Event) {
        self.primary_buffer
            .as_mut()
            .expect("no default output")
            .push(event);
    }

    pub fn push_named(&mut self, name: &str, event: Event) {
        self.named_buffers
            .get_mut(name)
            .expect("unknown output")
            .push(event);
    }

    pub fn append(&mut self, slice: &mut Vec<Event>) {
        self.primary_buffer
            .as_mut()
            .expect("no default output")
            .append(slice);
    }

    pub fn append_named(&mut self, name: &str, slice: &mut Vec<Event>) {
        self.named_buffers
            .get_mut(name)
            .expect("unknown output")
            .append(slice);
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.primary_buffer
            .as_mut()
            .expect("no default output")
            .drain()
    }

    pub fn drain_named(&mut self, name: &str) -> impl Iterator<Item = Event> + '_ {
        self.named_buffers
            .get_mut(name)
            .expect("unknown output")
            .drain()
    }

    pub fn take_primary(&mut self) -> OutputBuffer {
        std::mem::take(self.primary_buffer.as_mut().expect("no default output"))
    }

    pub fn take_all_named(&mut self) -> HashMap<String, OutputBuffer> {
        std::mem::take(&mut self.named_buffers)
    }

    pub fn len(&self) -> usize {
        self.primary_buffer.as_ref().map_or(0, OutputBuffer::len)
            + self
                .named_buffers
                .iter()
                .map(|(_, buf)| buf.len())
                .sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ByteSizeOf for TransformOutputsBuf {
    fn allocated_bytes(&self) -> usize {
        self.primary_buffer.size_of()
            + self
                .named_buffers
                .iter()
                .map(|(_, buf)| buf.size_of())
                .sum::<usize>()
    }
}

#[derive(Debug, Default)]
pub struct OutputBuffer(Vec<Event>);

impl OutputBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn push(&mut self, event: Event) {
        self.0.push(event);
    }

    pub fn append(&mut self, events: &mut Vec<Event>) {
        self.0.append(events);
    }

    pub fn extend(&mut self, events: impl Iterator<Item = Event>) {
        self.0.extend(events);
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.0.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn first(&self) -> Option<&Event> {
        self.0.first()
    }

    fn drain(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.0.drain(..)
    }

    async fn send(&mut self, output: &mut Fanout) {
        for event in self.0.drain(..) {
            output.feed(event).await.expect("unit error");
        }
        output.flush().await.expect("unit error");
    }

    pub fn into_events(self) -> impl Iterator<Item = Event> {
        self.0.into_iter()
    }
}

impl ByteSizeOf for OutputBuffer {
    fn allocated_bytes(&self) -> usize {
        self.0.iter().map(ByteSizeOf::size_of).sum()
    }
}

impl EventDataEq<Vec<Event>> for OutputBuffer {
    fn event_data_eq(&self, other: &Vec<Event>) -> bool {
        self.0.as_slice().event_data_eq(&other.as_slice())
    }
}

struct WrapEventTask<T>(T);

impl<T: TaskTransform<Event> + Send + 'static> TaskTransform<EventArray> for WrapEventTask<T> {
    fn transform(
        self: Box<Self>,
        stream: Pin<Box<dyn Stream<Item = EventArray> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = EventArray> + Send>> {
        // This is an aweful lot of boxes
        let stream = stream
            .flat_map(|events| stream::iter(events.into_events()))
            .boxed();
        Box::new(self.0).transform(stream).map(Into::into).boxed()
    }
}
