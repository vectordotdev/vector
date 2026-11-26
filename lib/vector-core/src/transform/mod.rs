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
    /// Create a synchronous transform with no private execution state.
    pub fn stateless(v: impl SyncTransform) -> Self {
        Self::stateful(Stateless(v))
    }

    /// Create a synchronous transform with private execution state.
    pub fn stateful(v: impl StatefulTransform) -> Self {
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
    /// Panics if the transform is synchronous.
    pub fn into_task(self) -> Box<dyn TaskTransform<EventArray>> {
        match self {
            Transform::Task(t) => t,
            Transform::Synchronous(_) => {
                panic!("Called `Transform::into_task` on something that was not a task variant.")
            }
        }
    }
}

/// Stateless transforms that write to the primary output.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a
///   `TaskTransform` or vice versa.
pub trait FunctionTransform: Send + Sync + 'static {
    /// Process an event and write any results to the primary output.
    fn transform(&self, output: &mut OutputBuffer, event: Event);
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

/// Stateless synchronous transforms that can write to multiple outputs.
pub trait SyncTransform: Send + Sync + 'static {
    /// Process an event and write any results to outputs declared by the transform.
    fn transform(&self, event: Event, output: &mut TransformOutputsBuf);
}

impl<T: FunctionTransform> SyncTransform for T {
    fn transform(&self, event: Event, output: &mut TransformOutputsBuf) {
        FunctionTransform::transform(
            self,
            output.primary_buffer.as_mut().expect("no default output"),
            event,
        );
    }
}

/// Synchronous transforms that keep their mutable execution state separate from their definition.
///
/// Inline execution retains state across arrays. Enable concurrent execution only when
/// starting each batch with fresh state preserves the transform's behavior.
pub trait StatefulTransform: Send + Sync + 'static {
    /// Mutable state owned by one execution.
    type State: Send + 'static;

    /// Initialize state once per inline executor or concurrent batch.
    fn new_state(&self) -> Self::State;

    /// Process an event and write any results to outputs declared by the transform.
    fn transform(&self, state: &mut Self::State, event: Event, output: &mut TransformOutputsBuf);

    /// Process an array of events with the current execution state.
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

struct Stateless<T>(T);

impl<T: SyncTransform> StatefulTransform for Stateless<T> {
    type State = ();

    fn new_state(&self) {}

    fn transform(&self, (): &mut (), event: Event, output: &mut TransformOutputsBuf) {
        self.0.transform(event, output);
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

impl<T: StatefulTransform> ErasedSyncTransform for T {
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

struct Executor<T: StatefulTransform> {
    definition: Arc<T>,
    state: T::State,
}

impl<T: StatefulTransform> SyncTransformExecutor for Executor<T> {
    fn transform_all(&mut self, events: EventArray, output: &mut TransformOutputsBuf) {
        self.definition
            .transform_all(&mut self.state, events, output);
    }
}

/// Precompute lineage transitions once per transform output, not once per event.
#[must_use]
pub fn schema_lineage_routes(
    output_id: &OutputId,
    definitions: &HashMap<OutputId, Arc<Definition>>,
) -> HashMap<OutputId, Arc<OutputId>> {
    definitions
        .keys()
        .map(|origin| (origin.clone(), Arc::new(origin.schema_route(output_id))))
        .collect()
}

#[allow(clippy::implicit_hasher)]
/// `event`: The event that will be updated
/// `output_id`: The `output_id` that the current even is being sent to (will be used as the new `parent_id`)
/// `log_schema_definitions`: A mapping of parent `OutputId` to definitions, that will be used to lookup the new runtime definition of the event
pub fn update_runtime_schema_definition(
    mut event: EventMutRef,
    output_id: &Arc<OutputId>,
    log_schema_definitions: &HashMap<OutputId, Arc<Definition>>,
    schema_routes: &HashMap<OutputId, Arc<OutputId>>,
) {
    if let EventMutRef::Log(log) = &mut event {
        // The upstream component changes on every hop. Resolve the schema
        // through its stable source output instead, retaining distinct meanings
        // when two sources feed a fan-in transform and then another transform.
        let lineage = log
            .metadata()
            .schema_origin()
            .filter(|id| log_schema_definitions.contains_key(*id))
            .or_else(|| {
                log.metadata()
                    .upstream_id()
                    .filter(|id| log_schema_definitions.contains_key(*id))
            });
        if let Some(lineage) = lineage {
            let definition = log_schema_definitions
                .get(lineage)
                .expect("lineage checked");
            let next_lineage = schema_routes.get(lineage).expect("route precomputed");
            log.metadata_mut().set_schema_definition(definition);
            log.metadata_mut()
                .set_schema_origin(Arc::clone(next_lineage));
        } else if let Some(definition) = log_schema_definitions.values().next() {
            // Transforms such as reduce can emit a new event with no source
            // lineage. Preserve a common schema without arbitrarily choosing
            // among differing source definitions.
            if log_schema_definitions
                .values()
                .all(|candidate| candidate == definition)
            {
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

    impl StatefulTransform for ObservedTransform {
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
    fn stateless_function_uses_primary_output_in_both_execution_modes() {
        struct Pass;

        impl FunctionTransform for Pass {
            fn transform(&self, output: &mut OutputBuffer, event: Event) {
                output.push(event);
            }
        }

        let Transform::Synchronous(transform) = Transform::stateless(Pass) else {
            panic!("expected a synchronous transform");
        };

        let mut batch_output = outputs();
        transform.transform_batch(vec![event().into(), event().into()], &mut batch_output);
        assert_eq!(batch_output.take_primary().len(), 2);

        let mut inline_output = outputs();
        let mut executor = transform.create_executor();
        executor.transform_all(event().into(), &mut inline_output);
        assert_eq!(inline_output.take_primary().len(), 1);
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
    fn schema_meanings_follow_source_through_two_transform_hops() {
        use crate::config::LogNamespace;
        use lookup::{OwnedTargetPath, owned_value_path};
        use vrl::value::Kind;

        let source_a = Arc::new(OutputId::from("source_a"));
        let source_b = Arc::new(OutputId::from("source_b"));
        let definition_a = Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy])
                .with_event_field(&owned_value_path!("host_a"), Kind::bytes(), Some("host")),
        );
        let definition_b = Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy])
                .with_event_field(&owned_value_path!("host_b"), Kind::bytes(), Some("host")),
        );
        let definitions = HashMap::from([
            (source_a.as_ref().clone(), definition_a),
            (source_b.as_ref().clone(), definition_b),
        ]);
        let first_hop = Arc::new(OutputId::from("fan_in"));
        let second_hop = Arc::new(OutputId::from("pass_through"));

        for (origin, expected_path) in [
            (
                source_a,
                OwnedTargetPath::event(owned_value_path!("host_a")),
            ),
            (
                source_b,
                OwnedTargetPath::event(owned_value_path!("host_b")),
            ),
        ] {
            let mut event = Event::Log(LogEvent::from("hello"));
            event.metadata_mut().set_schema_origin(Arc::clone(&origin));
            event.metadata_mut().set_upstream_id(origin);
            let first_routes = schema_lineage_routes(&first_hop, &definitions);
            update_runtime_schema_definition(
                event.as_mut_log().into(),
                &first_hop,
                &definitions,
                &first_routes,
            );
            assert_eq!(
                event.as_log().find_key_by_meaning("host"),
                Some(&expected_path)
            );
            assert_eq!(event.metadata().upstream_id(), Some(first_hop.as_ref()));

            let next_definitions = definitions
                .iter()
                .map(|(id, definition)| (id.schema_route(&first_hop), Arc::clone(definition)))
                .collect();
            let next_routes = schema_lineage_routes(&second_hop, &next_definitions);
            update_runtime_schema_definition(
                event.as_mut_log().into(),
                &second_hop,
                &next_definitions,
                &next_routes,
            );
            assert_eq!(
                event.as_log().find_key_by_meaning("host"),
                Some(&expected_path)
            );
            assert_eq!(event.metadata().upstream_id(), Some(second_hop.as_ref()));
        }
    }

    #[test]
    fn schema_meanings_follow_named_source_ports() {
        use crate::config::LogNamespace;
        use lookup::{OwnedTargetPath, owned_value_path};
        use vrl::value::Kind;

        let left = OutputId {
            component: "source".into(),
            port: Some("left".into()),
        };
        let right = OutputId {
            component: "source".into(),
            port: Some("right".into()),
        };
        let definitions = HashMap::from([
            (
                left.clone(),
                Arc::new(
                    Definition::new_with_default_metadata(
                        Kind::any_object(),
                        [LogNamespace::Legacy],
                    )
                    .with_event_field(
                        &owned_value_path!("left_host"),
                        Kind::bytes(),
                        Some("host"),
                    ),
                ),
            ),
            (
                right.clone(),
                Arc::new(
                    Definition::new_with_default_metadata(
                        Kind::any_object(),
                        [LogNamespace::Legacy],
                    )
                    .with_event_field(
                        &owned_value_path!("right_host"),
                        Kind::bytes(),
                        Some("host"),
                    ),
                ),
            ),
        ]);

        for (origin, expected_path) in [
            (left, OwnedTargetPath::event(owned_value_path!("left_host"))),
            (
                right,
                OwnedTargetPath::event(owned_value_path!("right_host")),
            ),
        ] {
            let mut event = Event::Log(LogEvent::from("hello"));
            let origin = Arc::new(origin);
            event.metadata_mut().set_schema_origin(Arc::clone(&origin));
            event.metadata_mut().set_upstream_id(origin);
            let mut current_definitions = definitions.clone();
            for hop in ["fan_in", "passthrough"] {
                let output = Arc::new(OutputId::from(hop));
                let routes = schema_lineage_routes(&output, &current_definitions);
                update_runtime_schema_definition(
                    event.as_mut_log().into(),
                    &output,
                    &current_definitions,
                    &routes,
                );
                assert_eq!(
                    event.as_log().find_key_by_meaning("host"),
                    Some(&expected_path)
                );
                current_definitions = current_definitions
                    .into_iter()
                    .map(|(id, def)| (id.schema_route(&output), def))
                    .collect();
            }
        }
    }

    #[test]
    fn reconverged_branch_definitions_keep_separate_host_meanings() {
        use crate::config::LogNamespace;
        use lookup::{OwnedTargetPath, owned_value_path};
        use vrl::value::Kind;

        let source = Arc::new(OutputId::from("source"));
        let left = Arc::new(OutputId::from("left"));
        let right = Arc::new(OutputId::from("right"));
        let merge = Arc::new(OutputId::from("merge"));

        let initial_definition = Arc::new(Definition::new_with_default_metadata(
            Kind::any_object(),
            [LogNamespace::Legacy],
        ));
        let original = HashMap::from([(source.as_ref().clone(), initial_definition)]);

        let left_definition = Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy])
                .with_event_field(&owned_value_path!("host_left"), Kind::bytes(), Some("host")),
        );
        let right_definition = Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy])
                .with_event_field(
                    &owned_value_path!("host_right"),
                    Kind::bytes(),
                    Some("host"),
                ),
        );

        let merged_input_definitions = HashMap::from([
            (source.schema_route(&left), left_definition),
            (source.schema_route(&right), right_definition),
        ]);
        assert_eq!(merged_input_definitions.len(), 2);
        let routes = schema_lineage_routes(&merge, &merged_input_definitions);

        for (branch, expected) in [
            (left, owned_value_path!("host_left")),
            (right, owned_value_path!("host_right")),
        ] {
            let mut event = Event::Log(LogEvent::from("hello"));
            event.metadata_mut().set_schema_origin(Arc::clone(&source));
            event.metadata_mut().set_upstream_id(Arc::clone(&source));
            let source_routes = schema_lineage_routes(&branch, &original);
            update_runtime_schema_definition(
                event.as_mut_log().into(),
                &branch,
                &original,
                &source_routes,
            );
            update_runtime_schema_definition(
                event.as_mut_log().into(),
                &merge,
                &merged_input_definitions,
                &routes,
            );
            assert_eq!(
                event.as_log().find_key_by_meaning("host"),
                Some(&OwnedTargetPath::event(expected)),
            );
        }
    }

    #[test]
    fn identityless_output_retains_a_definition_common_to_all_inputs() {
        use crate::config::LogNamespace;
        use lookup::{OwnedTargetPath, owned_value_path};
        use vrl::value::Kind;

        let definition = Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy])
                .with_event_field(&owned_value_path!("host_a"), Kind::bytes(), Some("host")),
        );
        let definitions = HashMap::from([
            (OutputId::from("source_a"), Arc::clone(&definition)),
            (OutputId::from("source_b"), definition),
        ]);
        let mut event = Event::Log(LogEvent::from("hello"));
        assert!(event.metadata().schema_origin().is_none());
        assert!(event.metadata().upstream_id().is_none());

        let output = Arc::new(OutputId::from("reduce"));
        let routes = schema_lineage_routes(&output, &definitions);
        update_runtime_schema_definition(event.as_mut_log().into(), &output, &definitions, &routes);
        assert_eq!(
            event.as_log().find_key_by_meaning("host"),
            Some(&OwnedTargetPath::event(owned_value_path!("host_a")))
        );
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
