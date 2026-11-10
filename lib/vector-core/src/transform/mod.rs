use std::{collections::HashMap, pin::Pin, sync::Arc};

use futures::{Stream, StreamExt};

use crate::{
    config::OutputId,
    event::{Event, EventArray, EventContainer, EventMutRef, into_event_stream},
    schema::Definition,
};

mod outputs;
#[cfg(feature = "lua")]
pub mod runtime_transform;

pub use outputs::{OutputBuffer, TransformOutputs, TransformOutputsBuf};

/// Transforms execute synchronously over events or asynchronously over a stream.
///
/// Synchronous definitions are shared; execution owns private mutable state.
/// Task transforms act as a coordination or barrier point.
pub enum Transform {
    Synchronous(Arc<dyn ErasedSyncTransform>),
    Task(Box<dyn TaskTransform<EventArray>>),
}

impl Transform {
    /// Create a new function transform.
    ///
    /// Mutable execution state is kept separately from the shared definition.
    ///
    /// **Note:** You should prefer to implement this over [`TaskTransform`]
    /// where possible.
    pub fn function(v: impl FunctionTransform + 'static) -> Self {
        Self::synchronous(v)
    }

    /// Create a new synchronous transform.
    ///
    /// This is a broader trait than the simple [`FunctionTransform`] in that it allows transforms
    /// to write to multiple outputs. Those outputs must be known in advanced and returned via
    /// `TransformConfig::outputs`. Attempting to send to any output not registered in advance is
    /// considered a bug and will cause a panic.
    pub fn synchronous(v: impl SyncTransform + 'static) -> Self {
        Transform::Synchronous(Arc::new(v))
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

    /// Transmute the inner transform into a task transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`FunctionTransform`] this will panic.
    pub fn into_task(self) -> Box<dyn TaskTransform<EventArray>> {
        match self {
            Transform::Task(t) => t,
            Transform::Synchronous(_) => {
                panic!("Called `Transform::into_task` on something that was not a task variant.")
            }
        }
    }
}

/// Single-output transforms with shared definitions and private execution state.
///
/// Stateful transforms can retain history when run inline. Enable concurrent
/// execution only when independently initialized batches preserve semantics.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a
///   `TaskTransform` or vice versa.
pub trait FunctionTransform: Send + Sync + 'static {
    /// Mutable state owned by one execution, never shared between concurrent batches.
    type State: Send + 'static;

    /// Initialize state for an inline executor or one concurrent batch.
    fn new_state(&self) -> Self::State;

    fn transform(&self, state: &mut Self::State, output: &mut OutputBuffer, event: Event);
}

/// Transforms that tend to be more complicated runtime style components.
///
/// These require coordination and map a stream of some `T` to some `U`.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a
///   `TaskTransform` or vice versa.
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
pub trait SyncTransform: Send + Sync + 'static {
    /// Mutable state owned by one execution. Stateless transforms use `()`.
    type State: Send + 'static;

    /// Initialize state once per inline executor or concurrent batch.
    fn new_state(&self) -> Self::State;

    fn transform(&self, state: &mut Self::State, event: Event, output: &mut TransformOutputsBuf);

    fn transform_all(
        &self,
        state: &mut Self::State,
        events: EventArray,
        output: &mut TransformOutputsBuf,
    ) {
        for event in events.into_events() {
            self.transform(state, event, output);
        }
    }
}

impl<T: FunctionTransform> SyncTransform for T {
    type State = T::State;

    fn new_state(&self) -> Self::State {
        FunctionTransform::new_state(self)
    }

    fn transform(&self, state: &mut Self::State, event: Event, output: &mut TransformOutputsBuf) {
        FunctionTransform::transform(
            self,
            state,
            output.primary_buffer.as_mut().expect("no default output"),
            event,
        );
    }
}

/// Object-safe topology interface to a shared synchronous transform definition.
///
/// State remains typed inside the adapter; concurrent execution needs neither a
/// boxed state nor a runtime downcast.
pub trait ErasedSyncTransform: Send + Sync {
    /// Process a whole compute batch with fresh state shared by all its arrays.
    fn transform_batch(&self, arrays: Vec<EventArray>, output: &mut TransformOutputsBuf);

    /// Bind the definition to persistent state for inline execution.
    fn create_executor(self: Arc<Self>) -> Box<dyn SyncTransformExecutor>;
}

impl<T: SyncTransform> ErasedSyncTransform for T {
    fn transform_batch(&self, arrays: Vec<EventArray>, output: &mut TransformOutputsBuf) {
        let mut state = self.new_state();
        for events in arrays {
            self.transform_all(&mut state, events, output);
        }
    }

    fn create_executor(self: Arc<Self>) -> Box<dyn SyncTransformExecutor> {
        let state = self.new_state();
        Box::new(Executor {
            definition: self,
            state,
        })
    }
}

/// A synchronous transform definition bound to private, persistent execution state.
pub trait SyncTransformExecutor: Send {
    fn transform_all(&mut self, events: EventArray, output: &mut TransformOutputsBuf);
}

struct Executor<T: SyncTransform> {
    definition: Arc<T>,
    state: T::State,
}

impl<T: SyncTransform> SyncTransformExecutor for Executor<T> {
    fn transform_all(&mut self, events: EventArray, output: &mut TransformOutputsBuf) {
        self.definition
            .transform_all(&mut self.state, events, output);
    }
}

#[allow(clippy::implicit_hasher)]
/// `event`: The event that will be updated
/// `output_id`: The `output_id` that the current even is being sent to (will be used as the new `parent_id`)
/// `log_schema_definitions`: A mapping of parent `OutputId` to definitions, that will be used to lookup the new runtime definition of the event
pub fn update_runtime_schema_definition(
    mut event: EventMutRef,
    output_id: &Arc<OutputId>,
    log_schema_definitions: &HashMap<OutputId, Arc<Definition>>,
) {
    if let EventMutRef::Log(log) = &mut event {
        if let Some(parent_component_id) = log.metadata().upstream_id() {
            if let Some(definition) = log_schema_definitions.get(parent_component_id) {
                log.metadata_mut().set_schema_definition(definition);
            }
        } else {
            // there is no parent defined. That means this event originated from a component that
            // isn't able to track the source, such as `reduce` or `lua`. In these cases, all of the
            // schema definitions _must_ be the same, so the first one is picked
            if let Some(definition) = log_schema_definitions.values().next() {
                log.metadata_mut().set_schema_definition(definition);
            }
        }
    }
    event.metadata_mut().set_upstream_id(Arc::clone(output_id));
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

    use std::{
        cell::Cell,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    // Deliberately not Clone; its state is Send but not Sync.
    #[derive(Default)]
    struct ObservedTransform {
        created: AtomicUsize,
        dropped: Arc<AtomicUsize>,
        observations: Mutex<Vec<(usize, usize, usize)>>,
    }

    struct ObservedState {
        id: usize,
        count: Cell<usize>,
        dropped: Arc<AtomicUsize>,
    }

    impl Drop for ObservedState {
        fn drop(&mut self) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl SyncTransform for ObservedTransform {
        type State = ObservedState;

        fn new_state(&self) -> ObservedState {
            ObservedState {
                id: self.created.fetch_add(1, Ordering::Relaxed),
                count: Cell::new(0),
                dropped: Arc::clone(&self.dropped),
            }
        }

        fn transform(
            &self,
            state: &mut ObservedState,
            event: Event,
            output: &mut TransformOutputsBuf,
        ) {
            state.count.set(state.count.get() + 1);
            self.observations.lock().unwrap().push((
                std::ptr::from_ref(self) as usize,
                state.id,
                state.count.get(),
            ));
            assert_ne!(event.as_log().get_message().unwrap(), &"panic".into());
            output.push(None, event);
        }
    }

    fn outputs() -> TransformOutputsBuf {
        TransformOutputsBuf::new_with_capacity(
            vec![crate::config::TransformOutput::new(
                crate::config::DataType::Log,
                HashMap::new(),
            )],
            1,
        )
    }

    fn event() -> Event {
        LogEvent::from("test").into()
    }

    #[test]
    fn batch_state_is_fresh_and_inline_state_persists() {
        let definition = Arc::new(ObservedTransform::default());
        let expected_address = Arc::as_ptr(&definition) as usize;
        let shared: Arc<dyn ErasedSyncTransform> = definition.clone();
        let mut output = outputs();
        shared.transform_batch(vec![event().into(), event().into()], &mut output);
        shared.transform_batch(vec![event().into()], &mut output);
        assert_eq!(definition.dropped.load(Ordering::Relaxed), 2);

        let mut executor = shared.create_executor();
        executor.transform_all(event().into(), &mut output);
        executor.transform_all(event().into(), &mut output);
        assert_eq!(definition.created.load(Ordering::Relaxed), 3);
        assert_eq!(definition.dropped.load(Ordering::Relaxed), 2);
        drop(executor);
        assert_eq!(definition.dropped.load(Ordering::Relaxed), 3);
        assert_eq!(output.take_primary().len(), 5);
        assert_eq!(
            *definition.observations.lock().unwrap(),
            [(0, 1), (0, 2), (1, 1), (2, 1), (2, 2)].map(|(id, count)| (
                expected_address,
                id,
                count
            )),
        );
    }

    #[test]
    fn concurrent_batches_share_definition_and_isolate_state() {
        let definition = Arc::new(ObservedTransform::default());
        let expected_address = Arc::as_ptr(&definition) as usize;
        let shared: Arc<dyn ErasedSyncTransform> = definition.clone();
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let shared = Arc::clone(&shared);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    shared.transform_batch(vec![event().into()], &mut outputs());
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(definition.created.load(Ordering::Relaxed), 4);
        assert_eq!(definition.dropped.load(Ordering::Relaxed), 4);
        let mut observations = definition.observations.lock().unwrap();
        observations.sort_unstable();
        assert_eq!(
            *observations,
            [0, 1, 2, 3].map(|id| (expected_address, id, 1))
        );
    }

    #[test]
    fn panicking_batch_drops_private_state() {
        let definition = ObservedTransform::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            definition.transform_batch(
                vec![Event::from(LogEvent::from("panic")).into()],
                &mut outputs(),
            );
        }));
        assert!(result.is_err());
        assert_eq!(definition.dropped.load(Ordering::Relaxed), 1);
    }

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
