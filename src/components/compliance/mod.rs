mod payloads;
mod resources;

use std::{
    future::{ready, Future},
    sync::{Arc, atomic::{AtomicUsize, Ordering}}, collections::VecDeque,
};

use futures_util::future::OptionFuture;
use tokio::{sync::{oneshot, mpsc::{self, error::SendError, OwnedPermit}, Barrier, Notify}, task::JoinSet, select};
use vector_core::event::EventArray;

use self::resources::HttpConfig;

pub enum ComponentType {
    Source,
    Transform,
    Sink,
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

pub struct ExternalResource {
    direction: ResourceDirection,
    resource: ResourceDefinition,
}

/// Validator input mechanism.
///
/// This is the mechanism by which the validator task pushes input to the component being validated.
pub struct ValidatorInput(mpsc::Sender<EventArray>);
/// Validator output mechanism.
///
/// This is the mechanism by which the validator task captures output from the component being
/// validated.
pub struct ValidatorOutput(mpsc::Receiver<EventArray>);

/// Component input mechanism.
///
/// This is the mechanism by which the component being validated captures its input from the
/// validator task.
pub enum ComponentInput {
    /// Component takes its input from other components.
    ///
    /// We link this channel to the output side of the validator task so that events pushed to this
    /// channel arrive directly at the component being validated.
    Channel(mpsc::Receiver<EventArray>),

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
    Channel(mpsc::Sender<EventArray>),

    /// Component pushes its output to an external resource.
    External,
}

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
            })
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
            return
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
    pub fn mark_as_done(self) {
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

pub struct ValidationResults;

pub struct ComplianceRunner;

impl ComplianceRunner {
    fn build_component_task<C: Component>(
        shutdown_handle: ShutdownHandle,
        input: ComponentInput,
        output: ComponentOutput,
    ) -> impl Future<Output = ()> {
        ready(())
    }

    fn spawn_external_input_resource(resource: ExternalResource, input_rx: mpsc::Receiver<EventArray>, tasks_started: &WaitGroup, tasks_completed: &WaitGroup) {
        
    }

    fn spawn_external_output_resource(resource: ExternalResource, output_tx: mpsc::Sender<EventArray>, tasks_started: &WaitGroup, tasks_completed: &WaitGroup) {
        
    }

    pub async fn validate<C: Component + 'static>() -> Result<(), Vec<String>> {
        // Build and spawn any necessary validator tasks that will drive the sending/receiving of events
        // related to the component being validated. We get back handles to all of those validator
        // tasks so we can ensure they shutdown cleanly, as well as a handle that can be used to
        // wait until all validator tasks have reported being "ready" -- ready to send data, or
        // receive it, etc -- before proceeding.
        let (input_tx, input_rx) = mpsc::channel(65_536);
        let (output_tx, output_rx) = mpsc::channel(65_536);
        let tasks_started = WaitGroup::new();
        let tasks_completed = WaitGroup::new();

        let (component_input, component_output) = match (C::component_type(), C::external_resource()) {
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
            // Transforms are simple: pure channel-based communication. The validator can simply
            // send events directly into the transform component, and receive any output events
            // directly as well.
            (ComponentType::Transform, None) => {
               (ComponentInput::Channel(input_rx), ComponentOutput::Channel(output_tx))
            },
            // Sources always have an external resource for their input side.
            //
            // We wire up the validator input to the external resource and the validator output to
            // the component.
            (ComponentType::Source, Some(resource)) => {
                Self::spawn_external_input_resource(resource, input_rx, &tasks_started, &tasks_completed);

                (ComponentInput::External, ComponentOutput::Channel(output_tx))
            },
            // Sinks always have an external resource for their output side.
            //
            // We wire up the validator output to the external resource and the validator input to
            // the component.
            (ComponentType::Sink, Some(resource)) => {
                Self::spawn_external_output_resource(resource, output_tx, &tasks_started, &tasks_completed);

                (ComponentInput::Channel(input_rx), ComponentOutput::External)
            },
        };

        // Build our component future, which is a wrapper around any necessary logic/behavior in
        // order to drive the component correctly.
        let (shutdown_trigger, shutdown_handle) = ShutdownTrigger::new();
        let component_future = Self::build_component_future::<C>(shutdown_handle, component_input, component_output);

        // Wait for our validator tasks, if any, to have started and signalled that they're ready.
        //
        // Once the validator tasks are ready, start driving our core validation loop, where we
        // inject events, drive the component itself to run and process those events, and then
        // collect all results at the end.
        tasks_started.wait_for_children().await;

        // Generate a number of payloads that will be fed as the input to the component, and create
        // a place to store the payloads we get back.
        let mut input_payloads = VecDeque::new();
        let mut output_payloads = Vec::new();

        let mut validator_input = Some(input_tx);
        let validator_output = &mut output_rx;
        let mut shutdown_trigger = Some(shutdown_trigger);

        #[derive(Debug, Clone, Copy, PartialEq)]
        enum ValidatorState {
            Running,
            InputDone,
            WaitingOnComponent,
            WaitingOnOutputs,
            Completed
        }

        impl ValidatorState {
            fn is_completed(self) -> bool {
                self == ValidatorState::Completed
            }

            fn is_component_active(self) -> bool {
                match self {
                    Self::Running | Self::InputDone | Self::WaitingOnComponent => true,
                    _ => false,
                }
            }
        }

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
        let mut validator_state = ValidatorState::Running;
        loop {
            let maybe_input_reserve: OptionFuture<Result<OwnedPermit<_>, SendError<()>>> = validator_input.take().map(|i| i.reserve_owned()).into();

            let maybe_new_validator_state = select! {
                // While our validator input is alive, try and reserve a slot to send the next input
                // to the component. As long as we have a sender, that implies we still have inputs
                // to send. When we have no more items to send, the permit we acquired will drop,
                // which in turn drops the channel and lets the component be signalled that the
                // input stream is now closed.
                //
                // We reserve our sending slot because otherwise we would risk losing inputs via
                // `send` if another future completed before the send did, and we additionally use
                // `reserve_owned` so that we can hold the sender in `Option<T>`, 
                maybe_permit = maybe_input_reserve, if !input_payloads.is_empty() => match maybe_permit {
                    Some(Err(_)) => panic!("validator input rx dropped unexpectedly"),
                    Some(Ok(permit)) => if let Some(input) = input_payloads.pop_front() {
                        debug!("Sent input from validator to component.");

                        // Send the input and re-arm our sender for the next send.
                        let sender = permit.send(input);
                        validator_input = Some(sender);

                        None
                    } else {
                        debug!("No input items left.");

                        // We got a permit but have no more inputs left to send, so transition to
                        // the `InputDone` state which will initiate the shutdown sequence for the component.
                        Some(ValidatorState::InputDone)
                    },
                    None => panic!("validator input channel should not be dropped before input payloads are all sent"),
                },

                // Drive the component if it is still actively processing.
                result = &mut component_future, if validator_state.is_component_active() => {
                    debug!("Component finished.");

                    // If the component has finished, transition to the `WaitingOnOutputs` state to
                    // signal that we should stop driving the component future and allow outputs to
                    // be fully drained.
                    Some(ValidatorState::WaitingOnOutputs)
                },

                // We got an output from the component, so stash it off to the side for now.
                maybe_output = validator_output.recv(), if !validator_state.is_completed() => match maybe_output {
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

            if let Some(new_validation_state) = maybe_new_validation_state {
                let existing_validation_state = validation_state;
                validation_state = new_validation_state;

                debug!("Validation state transitioned from {} to {}.", existing_validation_state, new_validation_state);
            }
        }

        Ok(())
    }
}
