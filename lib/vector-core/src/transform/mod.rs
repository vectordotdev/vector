use std::{collections::HashMap, error, pin::Pin};

use futures::{Stream, StreamExt};
use vector_common::internal_event::{
    self, register, CountByteSize, EventsSent, InternalEventHandle as _, Registered, DEFAULT_OUTPUT,
};
use vector_common::EventDataEq;

use crate::{
    config,
    event::{
        into_event_stream, EstimatedJsonEncodedSizeOf, Event, EventArray, EventContainer, EventRef,
    },
    fanout::{self, Fanout},
    ByteSizeOf,
};

#[cfg(any(feature = "lua"))]
pub mod runtime_transform;

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
            .flat_map(into_event_stream)
            .boxed()
    }
}

/// Broader than the simple [`FunctionTransform`], this trait allows transforms to write to
/// multiple outputs. Those outputs must be known in advanced and returned via
/// `TransformConfig::outputs`. Attempting to send to any output not registered in advance is
/// considered a bug and will cause a panic.
pub trait SyncTransform: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf);

    fn transform_all(&mut self, events: EventArray, output: &mut TransformOutputsBuf) {
        for event in events.into_events() {
            self.transform(event, output);
        }
    }
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

struct TransformOutput {
    fanout: Fanout,
    events_sent: Registered<EventsSent>,
}

pub struct TransformOutputs {
    outputs_spec: Vec<config::Output>,
    primary_output: Option<TransformOutput>,
    named_outputs: HashMap<String, TransformOutput>,
}

impl TransformOutputs {
    pub fn new(
        outputs_in: Vec<config::Output>,
    ) -> (Self, HashMap<Option<String>, fanout::ControlChannel>) {
        let outputs_spec = outputs_in.clone();
        let mut primary_output = None;
        let mut named_outputs = HashMap::new();
        let mut controls = HashMap::new();

        for output in outputs_in {
            let (fanout, control) = Fanout::new();
            match output.port {
                None => {
                    primary_output = Some(TransformOutput {
                        fanout,
                        events_sent: register(EventsSent::from(internal_event::Output(Some(
                            DEFAULT_OUTPUT.into(),
                        )))),
                    });
                    controls.insert(None, control);
                }
                Some(name) => {
                    named_outputs.insert(
                        name.clone(),
                        TransformOutput {
                            fanout,
                            events_sent: register(EventsSent::from(internal_event::Output(Some(
                                name.clone().into(),
                            )))),
                        },
                    );
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

    /// Sends the events in the buffer to their respective outputs.
    ///
    /// # Errors
    ///
    /// If an error occurs while sending events to their respective output, an error variant will be
    /// returned detailing the cause.
    pub async fn send(
        &mut self,
        buf: &mut TransformOutputsBuf,
    ) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        if let Some(primary) = self.primary_output.as_mut() {
            let count = buf.primary_buffer.as_ref().map_or(0, OutputBuffer::len);
            let byte_size = buf.primary_buffer.as_ref().map_or(
                0,
                EstimatedJsonEncodedSizeOf::estimated_json_encoded_size_of,
            );
            buf.primary_buffer
                .as_mut()
                .expect("mismatched outputs")
                .send(&mut primary.fanout)
                .await?;
            primary.events_sent.emit(CountByteSize(count, byte_size));
        }

        for (key, buf) in &mut buf.named_buffers {
            let count = buf.len();
            let byte_size = buf.estimated_json_encoded_size_of();
            let output = self.named_outputs.get_mut(key).expect("unknown output");
            buf.send(&mut output.fanout).await?;
            output.events_sent.emit(CountByteSize(count, byte_size));
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TransformOutputsBuf {
    primary_buffer: Option<OutputBuffer>,
    named_buffers: HashMap<String, OutputBuffer>,
}

impl TransformOutputsBuf {
    pub fn new_with_capacity(outputs_in: Vec<config::Output>, capacity: usize) -> Self {
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

    pub fn extend(&mut self, events: impl Iterator<Item = Event>) {
        self.primary_buffer
            .as_mut()
            .expect("no default output")
            .extend(events);
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
                .values()
                .map(OutputBuffer::len)
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
                .values()
                .map(ByteSizeOf::size_of)
                .sum::<usize>()
    }
}

#[derive(Debug, Default, Clone)]
pub struct OutputBuffer(Vec<EventArray>);

impl OutputBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn push(&mut self, event: Event) {
        // Coalesce multiple pushes of the same type into one array.
        match (event, self.0.last_mut()) {
            (Event::Log(log), Some(EventArray::Logs(logs))) => {
                logs.push(log);
            }
            (Event::Metric(metric), Some(EventArray::Metrics(metrics))) => {
                metrics.push(metric);
            }
            (Event::Trace(trace), Some(EventArray::Traces(traces))) => {
                traces.push(trace);
            }
            (event, _) => {
                self.0.push(event.into());
            }
        }
    }

    pub fn append(&mut self, events: &mut Vec<Event>) {
        for event in events.drain(..) {
            self.push(event);
        }
    }

    pub fn extend(&mut self, events: impl Iterator<Item = Event>) {
        for event in events {
            self.push(event);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(EventArray::len).sum()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn first(&self) -> Option<EventRef> {
        self.0.first().and_then(|first| match first {
            EventArray::Logs(l) => l.first().map(Into::into),
            EventArray::Metrics(m) => m.first().map(Into::into),
            EventArray::Traces(t) => t.first().map(Into::into),
        })
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.0.drain(..).flat_map(EventArray::into_events)
    }

    async fn send(
        &mut self,
        output: &mut Fanout,
    ) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        for array in std::mem::take(&mut self.0) {
            output.send(array).await?;
        }

        Ok(())
    }

    fn iter_events(&self) -> impl Iterator<Item = EventRef> {
        self.0.iter().flat_map(EventArray::iter_events)
    }

    pub fn into_events(self) -> impl Iterator<Item = Event> {
        self.0.into_iter().flat_map(EventArray::into_events)
    }

    pub fn take_events(&mut self) -> Vec<EventArray> {
        std::mem::take(&mut self.0)
    }
}

impl ByteSizeOf for OutputBuffer {
    fn allocated_bytes(&self) -> usize {
        self.0.iter().map(ByteSizeOf::size_of).sum()
    }
}

impl EventDataEq<Vec<Event>> for OutputBuffer {
    fn event_data_eq(&self, other: &Vec<Event>) -> bool {
        struct Comparator<'a>(EventRef<'a>);

        impl<'a> PartialEq<&Event> for Comparator<'a> {
            fn eq(&self, that: &&Event) -> bool {
                self.0.event_data_eq(that)
            }
        }

        self.iter_events().map(Comparator).eq(other.iter())
    }
}

impl EstimatedJsonEncodedSizeOf for OutputBuffer {
    fn estimated_json_encoded_size_of(&self) -> usize {
        self.0
            .iter()
            .map(EstimatedJsonEncodedSizeOf::estimated_json_encoded_size_of)
            .sum()
    }
}

impl From<Vec<Event>> for OutputBuffer {
    fn from(events: Vec<Event>) -> Self {
        let mut result = Self::default();
        result.extend(events.into_iter());
        result
    }
}

struct WrapEventTask<T>(T);

impl<T: TaskTransform<Event> + Send + 'static> TaskTransform<EventArray> for WrapEventTask<T> {
    fn transform(
        self: Box<Self>,
        stream: Pin<Box<dyn Stream<Item = EventArray> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = EventArray> + Send>> {
        // This is an awful lot of boxes
        let stream = stream.flat_map(into_event_stream).boxed();
        Box::new(self.0).transform(stream).map(Into::into).boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::{LogEvent, Metric, MetricKind, MetricValue};

    #[test]
    fn buffers_output() {
        let mut buf = OutputBuffer::default();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.0.len(), 0);

        // Push adds a new element
        buf.push(LogEvent::default().into());
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.0.len(), 1);

        // Push of the same type adds to the existing element
        buf.push(LogEvent::default().into());
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.0.len(), 1);

        // Push of a different type adds a new element
        buf.push(
            Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )
            .into(),
        );
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.0.len(), 2);

        // And pushing again adds a new element
        buf.push(LogEvent::default().into());
        assert_eq!(buf.len(), 4);
        assert_eq!(buf.0.len(), 3);
    }
}
