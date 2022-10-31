mod payloads;
mod resources;

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    iter,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use tokio::{
    pin, select,
    sync::{oneshot, Barrier, Notify},
};
use vector_buffers::topology::channel::{limited, LimitedReceiver, LimitedSender};
use vector_common::{config::ComponentKey, shutdown::ShutdownSignal};
use vector_core::{
    config::{proxy::ProxyConfig, GlobalOptions},
    event::{Event, EventArray, LogEvent},
    sink::VectorSink,
    source::Source,
    transform::Transform,
};

use crate::{
    config::{schema, SinkContext, SinkHealthcheckOptions, SourceContext, TransformContext},
    SourceSender,
};

use self::resources::HttpConfig;

pub enum ComponentType {
    Source,
    Transform,
    Sink,
}

pub enum ComponentBuilderParts {
    Source(SourceContext),
    Transform(TransformContext),
    Sink(SinkContext),
}

pub enum BuiltComponent {
    Source(Source),
    Transform(Transform),
    Sink(VectorSink),
}

impl BuiltComponent {
    fn into_source_component(self) -> Source {
        match self {
            Self::Source(source) => source,
            _ => panic!("source component returned built component of different type"),
        }
    }

    fn into_transform_component(self) -> Transform {
        match self {
            Self::Transform(transform) => transform,
            _ => panic!("transform component returned built component of different type"),
        }
    }

    fn into_sink_component(self) -> VectorSink {
        match self {
            Self::Sink(sink) => sink,
            _ => panic!("sink component returned built component of different type"),
        }
    }
}

pub enum ResourceDirection {
    Pull,
    Push,
}

pub enum ResourceDefinition {
    Http(HttpConfig),
}

pub enum ComponentPayload {
    Json,
}

impl ComponentPayload {
    pub fn into_event(self) -> Event {
        match self {
            Self::Json => {
                // Dummy event for now.
                let mut log = LogEvent::default();
                log.insert("field_a", "value_a");
                log.insert("field_b.subfield_a", 42);
                log.insert("field_b.subfield_b", vec!["foo", "bar"]);
                log.into()
            }
        }
    }
}

pub struct ExternalResource {
    direction: ResourceDirection,
    resource: ResourceDefinition,
}

/// Validator input mechanism.
///
/// This is the mechanism by which the validator task pushes input to the component being validated.
pub struct ValidatorInput(LimitedSender<EventArray>);
/// Validator output mechanism.
///
/// This is the mechanism by which the validator task captures output from the component being
/// validated.
pub struct ValidatorOutput(LimitedReceiver<EventArray>);

/// Component input mechanism.
///
/// This is the mechanism by which the component being validated captures its input from the
/// validator task.
pub enum ComponentInput {
    /// Component takes its input from other components.
    ///
    /// We link this channel to the output side of the validator task so that events pushed to this
    /// channel arrive directly at the component being validated.
    Channel(LimitedSender<EventArray>),

    /// Component takes its input from an external resource.
    External,
}

/// Component output mechanism.
///
/// This is the mechanism by which the component being validated pushes its output to the validator
/// task.
pub enum ComponentOutput {
    /// Component sends its output to other components.
    ///
    /// We link this channel to the input side of the validator task so that events pushed to this
    /// channel arrive directly at the validator task.
    Channel(LimitedReceiver<EventArray>),

    /// Component pushes its output to an external resource.
    External,
}

#[async_trait]
pub trait Component {
    /// Gets the type of the component.
    fn component_type() -> ComponentType;

    /// Gets the external resource associated with this component.
    ///
    /// For sources and sinks, there is always an "external" resource, whether it's an address to
    /// listen on for traffic, or a Kafka cluster to send events to, and so on. `ExternalResource`
    /// defines what that external resource is in a semi-structured way, including the
    /// directionality i.e. pull vs push.
    ///
    /// Components inherently have their external resource either as an input (source) or an output
    /// (sink). For transforms, they are attached to components on both sides, so they require no
    /// external resource.
    // TODO: Should this be a vector for multiple resources? Does anything actually have multiple
    // external resource dependencies? Not necessarily in the sense of, say, the `file` source
    // monitoring multiple files, but a component that both listens on a TCP socket _and_ opens a
    // specific file, etc.
    fn external_resource() -> Option<ExternalResource>;

    /// Gets the type of payload that should be sent into the component.
    ///
    /// Automatic conversion is handled as necessary: payloads are used in their raw form when sent
    /// to sources, or convert to an equivalent internal event representation when sent to a
    /// transform or sink, and so on.
    fn payload() -> ComponentPayload;

    /// Builds a future that represents the runnable portion of a component.
    ///
    /// Given that this trait covers multiple component types, `ComponentBuilderParts` provides an
    /// opaque set of component type-specific parts needed for building a component. If the builder
    /// parts do not match the actual component type, `Err(...)` is returned with an error
    /// describing this. Alternatively, if the builder parts are correct but there is a general
    /// error with building the component, `Err(...)` is also returned.
    ///
    /// Otherwise, `Ok(...)` is returned, containing the built component.
    async fn build_component(
        builder_parts: ComponentBuilderParts,
    ) -> Result<BuiltComponent, String>;
}

struct WaitGroupState {
    outstanding: AtomicUsize,
    notify: Notify,
}

/// A synchronization primitive for waiting for an arbitrary number of processes to rendezvous.
struct WaitGroup {
    state: Arc<WaitGroupState>,
}

struct WaitGroupChild {
    done: bool,
    state: Arc<WaitGroupState>,
}

impl WaitGroup {
    pub fn new() -> Self {
        Self {
            state: Arc::new(WaitGroupState {
                outstanding: AtomicUsize::new(1),
                notify: Notify::new(),
            }),
        }
    }

    pub fn add_child(&self) -> WaitGroupChild {
        WaitGroupChild::from_state(&self.state)
    }

    /// Waits until all children have marked themselves as done.
    ///
    /// If no children were added to the wait group, or all of them have already completed, this
    /// function returns immediately.
    pub async fn wait_for_children(self) {
        // The `outstanding` count starts at 1 when we create the `WaitGroup`. If we create a child
        // handle and `outstanding` went from 0 to 1, and then the task uses the handle finished
        // _before_ we called `wait_for_children`, it would think it should trigger a notification,
        // having theoretically reached zero aka "all children are done".
        //
        // This way, handles can't trigger the notification before we're actually waiting. We check
        // `outstanding` here and see if the value is one, which would mean that we had no children
        // or they've all completed, and we can short-circuit waiting to be notified entirely.
        let previous = self.state.outstanding.load(Ordering::Acquire);
        if previous == 1 {
            return;
        }

        self.state.notify.notified().await
    }
}

impl WaitGroupChild {
    pub fn from_state(state: &Arc<WaitGroupState>) -> Self {
        state.outstanding.fetch_add(1, Ordering::Release);

        Self {
            done: false,
            state: Arc::clone(state),
        }
    }

    /// Marks this child as done.
    ///
    /// If the wait group has been finalized and is waiting for all children to be marked as done,
    /// and this is the last outstanding child to be marked as done, the wait group will be notified.
    pub fn mark_as_done(mut self) {
        self.done = true;

        // Decrement the `outstanding` count, and if the value is now at zero, that means the wait
        // group is currently waiting for all children to mark themselves as done, and we're the
        // last child... and so we're responsible for notifying the wait group.
        let previous = self.state.outstanding.fetch_sub(1, Ordering::Release);
        if previous == 1 {
            self.state.notify.notify_waiters();
        }
    }
}

impl Drop for WaitGroupChild {
    fn drop(&mut self) {
        if !self.done {
            panic!("wait group child dropped without being marked as done");
        }
    }
}

struct ShutdownTrigger {
    shutdown_tx: oneshot::Sender<()>,
    barrier: Arc<Barrier>,
}

struct ShutdownHandle {
    shutdown_rx: Option<oneshot::Receiver<()>>,
    barrier: Arc<Barrier>,
}

impl ShutdownTrigger {
    pub fn new() -> (Self, ShutdownHandle) {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let barrier = Arc::new(Barrier::new(2));

        let trigger = Self {
            shutdown_tx,
            barrier: Arc::clone(&barrier),
        };

        let handle = ShutdownHandle {
            shutdown_rx: Some(shutdown_rx),
            barrier,
        };

        (trigger, handle)
    }

    /// Triggers the shutdown process and waits for the paired task to signal that they have shutdown.
    pub async fn trigger_and_wait(self) {
        self.shutdown_tx
            .send(())
            .expect("paired task should not yet be shutdown");
        self.barrier.wait().await;
    }
}

impl ShutdownHandle {
    /// Waits for the signal to shutdown.
    pub async fn wait_for_shutdown(&mut self) {
        match self.shutdown_rx.as_mut() {
            Some(rx) => rx.await.expect("paired task should not yet be shutdown"),
            None => panic!("tried to await shutdown signal but has already been received"),
        }

        // If we're here, we've successfully received the shutdown signal, so we consume the
        // receiver, as it cannot be used/polled again.
        self.shutdown_rx.take();
    }

    /// Marks the task as done.
    ///
    /// This synchronizes with the paired task via a barrier, such that both this task and the
    /// paired task will not proceed past the barrier until both have reached it.
    pub async fn mark_as_done(self) {
        self.barrier.wait().await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ValidatorState {
    Running,
    InputDone,
    WaitingOnComponent,
    WaitingOnOutputs,
    Completed,
}

impl ValidatorState {
    fn is_running(self) -> bool {
        self == ValidatorState::Running
    }

    fn is_component_active(self) -> bool {
        match self {
            Self::Running | Self::InputDone | Self::WaitingOnComponent => true,
            _ => false,
        }
    }

    fn is_completed(self) -> bool {
        self == ValidatorState::Completed
    }
}

pub struct ValidatorResults {
    inputs: Vec<EventArray>,
    outputs: Vec<EventArray>,
}

pub struct Validator<C> {
    _c: PhantomData<C>,
}

impl<C: Component + 'static> Validator<C> {
    async fn build_source_component_task(
        &mut self,
        mut shutdown_handle: ShutdownHandle,
        resource: ExternalResource,
        tasks_started: &WaitGroup,
        tasks_completed: &WaitGroup,
    ) -> (
        Pin<Box<dyn Future<Output = ()>>>,
        ValidatorInput,
        ValidatorOutput,
    ) {
        // First we'll spawn the external input resource. We ensure that the external resource is
        // ready via `tasks_started` when the validator actually runs.
        let (input_tx, input_rx) = limited(1024);
        self.spawn_external_input_resource(resource, input_rx, tasks_started, tasks_completed);

        // Now actually build the source itself. We end up wrapping it in a very thin layer of glue to
        // drive it properly and ensure that we trigger it to shutdown when the validator tells us
        // that it's time to shutdown from its perspective.
        let (source_tx, validator_rx) = SourceSender::new_with_buffer(1024);
        let (shutdown_trigger, shutdown, _) = ShutdownSignal::new_wired();
        let source_context = SourceContext {
            key: ComponentKey::from("validator_source"),
            globals: GlobalOptions::default(),
            shutdown,
            out: source_tx,
            proxy: ProxyConfig::default(),
            acknowledgements: true,
            schema: schema::Options::default(),
            schema_definitions: HashMap::new(),
        };

        let source_builder_parts = ComponentBuilderParts::Source(source_context);
        let source_component = C::build_component(source_builder_parts)
            .await
            .expect("failed to build source component")
            .into_source_component();

        let fut = Box::pin(async move {
            let mut shutdown_trigger = Some(shutdown_trigger);
            pin!(source_component);

            loop {
                select! {
                    // Wait for the shutdown signal from the validator, and then trigger
                    // shutdown of the source with its native shutdown signal.
                    _ = shutdown_handle.wait_for_shutdown(), if shutdown_trigger.is_some() => {
                        drop(shutdown_trigger.take());
                    },

                    // Drive the source component until it completes, in which case we're done. This
                    // should really only occur once we've triggered shutdown.
                    // TODO: Do something with the result of `source_component`.
                    _result = &mut source_component => {
                        if shutdown_trigger.is_some() {
                            panic!("source component completed prior to shutdown being triggered");
                        }

                        break
                    },
                }
            }

            shutdown_handle.mark_as_done().await;
        });

        (fut, ValidatorInput(input_tx), ValidatorOutput(validator_rx))
    }

    async fn build_transform_component_task(
        &mut self,
        shutdown_handle: ShutdownHandle,
    ) -> (
        Pin<Box<dyn Future<Output = ()>>>,
        ValidatorInput,
        ValidatorOutput,
    ) {
        // As transforms have no external resources, we simply build the transform component and
        // wrap it so that we can drive it depending on which specific type of transform it is.
        let (input_tx, _input_rx) = limited(1024);
        let (_output_tx, output_rx) = limited(1024);

        let transform_context = TransformContext::default();
        let transform_builder_parts = ComponentBuilderParts::Transform(transform_context);
        let transform_component = C::build_component(transform_builder_parts)
            .await
            .expect("failed to build transform component")
            .into_transform_component();

        let fut = Box::pin(async move {
            match transform_component {
                Transform::Function(_ft) => {}
                Transform::Synchronous(_st) => (),
                Transform::Task(_tt) => {}
            };

            shutdown_handle.mark_as_done().await;
        });

        (fut, ValidatorInput(input_tx), ValidatorOutput(output_rx))
    }

    async fn build_sink_component_task(
        &mut self,
        shutdown_handle: ShutdownHandle,
        resource: ExternalResource,
        tasks_started: &WaitGroup,
        tasks_completed: &WaitGroup,
    ) -> (
        Pin<Box<dyn Future<Output = ()>>>,
        ValidatorInput,
        ValidatorOutput,
    ) {
        // First we'll spawn the external output resource. We ensure that the external resource is
        // ready via `tasks_started` when the validator actually runs.
        let (input_tx, input_rx) = limited(1024);
        let (output_tx, validator_rx) = limited(1024);
        self.spawn_external_output_resource(resource, output_tx, tasks_started, tasks_completed);

        // Now actually build the sink itself. We end up wrapping it in a very thin layer of glue to
        // drive it properly and mark when the component completes.
        let sink_context = SinkContext {
            healthcheck: SinkHealthcheckOptions::default(),
            globals: GlobalOptions::default(),
            proxy: ProxyConfig::default(),
            schema: schema::Options::default(),
        };

        let sink_builder_parts = ComponentBuilderParts::Sink(sink_context);
        let sink_component = C::build_component(sink_builder_parts)
            .await
            .expect("failed to build sink component")
            .into_sink_component();

        let fut = Box::pin(async move {
            // TODO: Do something with the result of `VectorSink::run`.
            let _result = sink_component.run(input_rx.into_stream()).await;
            shutdown_handle.mark_as_done().await;
        });

        (fut, ValidatorInput(input_tx), ValidatorOutput(validator_rx))
    }

    async fn build_component_task(
        &mut self,
        shutdown_handle: ShutdownHandle,
        tasks_started: &WaitGroup,
        tasks_completed: &WaitGroup,
    ) -> (
        Pin<Box<dyn Future<Output = ()>>>,
        ValidatorInput,
        ValidatorOutput,
    ) {
        match (C::component_type(), C::external_resource()) {
            // Sources and sinks implicitly must have an external resource configured, regardless of
            // whether or not the resource is push or pull.
            (ComponentType::Source, None) | (ComponentType::Sink, None) => {
                panic!("sources and sinks must always have an external resource declared")
            }
            // Likewise, transforms are always in the "inside", so they should never have an
            // external resource defined.
            (ComponentType::Transform, Some(_)) => {
                panic!("transforms should never have an external resource declared")
            }
            (ComponentType::Source, Some(resource)) => {
                self.build_source_component_task(
                    shutdown_handle,
                    resource,
                    &tasks_started,
                    &tasks_completed,
                )
                .await
            }
            (ComponentType::Transform, None) => {
                self.build_transform_component_task(shutdown_handle).await
            }
            (ComponentType::Sink, Some(resource)) => {
                self.build_sink_component_task(
                    shutdown_handle,
                    resource,
                    &tasks_started,
                    &tasks_completed,
                )
                .await
            }
        }
    }

    fn spawn_external_input_resource(
        &mut self,
        _resource: ExternalResource,
        _input_rx: LimitedReceiver<EventArray>,
        _tasks_started: &WaitGroup,
        _tasks_completed: &WaitGroup,
    ) {
    }

    fn spawn_external_output_resource(
        &mut self,
        _resource: ExternalResource,
        _output_tx: LimitedSender<EventArray>,
        _tasks_started: &WaitGroup,
        _tasks_completed: &WaitGroup,
    ) {
    }

    fn generate_input_payloads(&self) -> VecDeque<EventArray> {
        let component_payload = C::payload();
        iter::once(component_payload.into_event())
            .cycle()
            .take(3)
            .enumerate()
            .map(|(i, event)| {
                if let Event::Log(mut log) = event {
                    log.insert("event_id", i);
                    log.into()
                } else {
                    event.into()
                }
            })
            .collect()
    }

    pub async fn validate(mut self) -> Result<ValidatorResults, Vec<String>> {
        // Build and spawn any necessary validator tasks that will drive the sending/receiving of events
        // related to the component being validated. We get back handles to all of those validator
        // tasks so we can ensure they shutdown cleanly, as well as a handle that can be used to
        // wait until all validator tasks have reported being "ready" -- ready to send data, or
        // receive it, etc -- before proceeding.
        let tasks_started = WaitGroup::new();
        let tasks_completed = WaitGroup::new();

        // Build our component future, which gets us the resulting input/output objects necessary to
        // drive it. We also spawn any external resource necessary for the given component, which
        // also gets us the input/output objects necessary to send input payloads, or receive output
        // payloads.
        let (shutdown_trigger, shutdown_handle) = ShutdownTrigger::new();
        let (component_future, mut validator_input, mut validator_output) = self
            .build_component_task(shutdown_handle, &tasks_started, &tasks_completed)
            .await;

        // diagrams:
        //
        // source:
        //   [           channel #1           ]             [            channel #2            ]
        //   [  channel tx  ]    [ channel rx ]             [  channel tx ]     [  channel rx  ]
        //   ( validator tx ) -> (  external  ) -> ( source (  source tx  )) -> ( validator rx )
        //
        // transform:
        //   [            channel #1             ]           [            channel #2             ]
        //   [  channel tx  ]     [  channel rx  ]           [  channel tx  ]     [  channel rx  ]
        //   ( validator tx ) -> (( transform rx ) transform ( transform tx )) -> ( validator rx )
        //
        // sink:
        //   [           channel #1            ]           [           channel #2           ]
        //   [  channel tx  ]     [ channel rx ]           [ channel tx ]    [  channel rx  ]
        //   ( validator tx ) -> ((   sink rx  ) sink ) -> (  external  ) -> ( validator rx )

        // Wait for our validator tasks, if any, to have started and signalled that they're ready.
        //
        // Once the validator tasks are ready, start driving our core validation loop, where we
        // inject events, drive the component itself to run and process those events, and then
        // collect all results at the end.
        tasks_started.wait_for_children().await;

        // Our core logic is straightforward: send inputs from the validator to the component, let
        // the component process them, and have the validator collect any outputs from the
        // component.
        //
        // We have various phases where we'll synchronize with the various pieces:
        // - when we're done sending our inputs, we trigger a shutdown and close our inputs channel
        // - being told to shutdown / having the input stream come to an end, the component should
        //   finish up and do its remaining processing
        // - once the component is done, we collect any remaining outputs and then we're done
        //
        // Below, we have this logic represented as a small state machine loop that ensures we're
        // sending inputs to the component, driving the component so it can process those inputs and
        // possibly generate outputs, and then collecting those outputs.
        let mut input_payloads = self.generate_input_payloads();
        let input_payloads_result = Vec::from_iter(input_payloads.iter().cloned());
        let mut output_payloads = Vec::new();

        let mut shutdown_trigger = Some(shutdown_trigger);
        let mut input_task_handle = tokio::spawn(async move {
            while let Some(input) = input_payloads.pop_front() {
                validator_input
                    .0
                    .send(input)
                    .await
                    .expect("should not fail to send validator input to component");
                debug!("Sent input from validator to component.");
            }

            debug!("No input items left.");
        });

        pin!(component_future);

        // TODO: We need a way to signal to the external resource to close when we're all done.
        //
        // Generally, we can figure this out based on the position of the external resource: if it's
        // driving a source, it should be complete when our validator input channel closes, and if
        // it's driving a sink, it should be finished when the sink finishes.
        //
        // This is mostly important for sinks where we need to be able to meaningfully signal to the
        // external output resource that the sink is done, and so it should flush whatever it has left,
        // etc, if it needs to do anything like that... and then close.
        //
        // The actual completion of the external resource can funnel back to us in one of two ways:
        // the wait group completes if we wait on it, or the output channel closes and no items are
        // left. The former is how we'd detect an external input resource finishing, and the latter
        // is how we'd detect an external output resource finishing.
        //
        // Our core logic loop, however, is a bit more agnostic, so we need a way to almost
        // idempotently be able to signal the external resource to shutdown both after we finish
        // sending all of our inputs _and_ after the component completes, where the external
        // resource would only pay attention to the usage that mattered for itself i.e. an external
        // output resource only caring about the shutdown signal sent when a sink component
        // completes.. or we could just generate a shutdown trigger for both and fire them blindly,
        // and let the "spawn the external resource" code chose which one it needs to hold on to/pay
        // attention to. :shrug:

        let mut validator_state = ValidatorState::Running;
        loop {
            let maybe_new_validator_state = select! {
                // Drive our input sender until it's done sending all of the inputs.
                result = &mut input_task_handle, if validator_state.is_running() => match result {
                    Ok(()) => Some(ValidatorState::InputDone),
                    Err(_) => panic!("Validator input sender task panicked unexpectedly."),
                },

                // Drive the component if it is still actively processing.
                _ = &mut component_future, if validator_state.is_component_active() => {
                    debug!("Component finished.");

                    // If the component has finished, transition to the `WaitingOnOutputs` state to
                    // signal that we should stop driving the component future and allow outputs to
                    // be fully drained.
                    Some(ValidatorState::WaitingOnOutputs)
                },

                // We got an output from the component, so stash it off to the side for now.
                maybe_output = validator_output.0.next(), if !validator_state.is_completed() => match maybe_output {
                    Some(output) => {
                        debug!("Got output item from component.");

                        output_payloads.push(output);
                        None
                    },

                    // The channel can only be closed when the component has completed, so if we're here, mark
                    // ourselves as having completed.
                    None => {
                        debug!("Component output channel closed.");

                        Some(ValidatorState::Completed)
                    },
                },

                // None of our branches matched, so figure out what our current state is, and
                // based on our progress so far, if we need to transition to the next state.
                else => match validator_state {
                    // We're still running, no change.
                    ValidatorState::Running => None,

                    // Our input stage marked itself as done, so we need to signal the component
                    // to shutdown, beginning the overall shutdown process.
                    ValidatorState::InputDone => {
                        debug!("Input stage done, triggering component shutdown...");

                        shutdown_trigger.take().expect("shutdown trigger already taken").trigger_and_wait().await;

                        Some(ValidatorState::WaitingOnComponent)
                    },

                    // We need to wait for the component to actually finish to transition out of
                    // this state.
                    ValidatorState::WaitingOnComponent => None,

                    // We're still waiting on any remaining outputs to be drained and for the
                    // output channel to be closed.
                    ValidatorState::WaitingOnOutputs => None,

                    // The validation run has completed, so we're down driving our state machine.
                    ValidatorState::Completed => {
                        debug!("Validation complete, breaking.");

                        break
                    },
                }
            };

            if let Some(new_validator_state) = maybe_new_validator_state {
                let existing_validator_state = validator_state;
                validator_state = new_validator_state;

                debug!(
                    "Validator state transitioned from {:?} to {:?}.",
                    existing_validator_state, new_validator_state
                );
            }
        }

        Ok(ValidatorResults {
            inputs: input_payloads_result,
            outputs: output_payloads,
        })
    }
}

impl<C: Component + 'static> Default for Validator<C> {
    fn default() -> Self {
        Self { _c: PhantomData }
    }
}

#[cfg(test)]
mod tests {
    use vector_core::{
        event::Event,
        transform::{FunctionTransform, OutputBuffer},
    };

    use super::*;

    // A simple transform that just forwards its event untouched.
    #[derive(Clone)]
    struct ValidatableTransform;

    impl FunctionTransform for ValidatableTransform {
        fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
            output.push(event);
        }
    }

    #[async_trait]
    impl Component for ValidatableTransform {
        fn component_type() -> ComponentType {
            ComponentType::Transform
        }

        fn external_resource() -> Option<ExternalResource> {
            None
        }

        fn payload() -> ComponentPayload {
            ComponentPayload::Json
        }

        async fn build_component(
            _builder_parts: ComponentBuilderParts,
        ) -> Result<BuiltComponent, String> {
            Ok(BuiltComponent::Transform(Transform::Function(Box::new(
                ValidatableTransform,
            ))))
        }
    }

    #[tokio::test]
    async fn basic() {
        let validator = Validator::<ValidatableTransform>::default();
        let results = validator.validate().await;
        assert!(results.is_ok());

        let results = results.expect("results should be ok");
        assert_eq!(results.inputs, results.outputs);
    }
}
