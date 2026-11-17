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
