mod config;
mod io;
mod telemetry;

use std::{collections::HashMap, time::Duration};

use tokio::{runtime::Builder, select, sync::mpsc};
use vector_core::event::Event;

use crate::{config::ConfigDiff, topology};

use super::{
    sync::{Configuring, TaskCoordinator},
    ComponentType, TestCaseExpectation, ValidatableComponent, Validator, WaitHandle,
};

use self::config::TopologyBuilder;

/// Runner input mechanism.
///
/// This is the mechanism by which the runner task pushes input to the component being validated.
pub enum RunnerInput {
    /// The component uses an external resource for its input.
    ///
    /// The channel provides a mechanism to send inputs to the external resource, which is then
    /// either pulled from by the component, or pushes directly to the component.
    ///
    /// Only sources have external inputs.
    External(mpsc::Sender<Event>),

    /// The component uses a "controlled" edge for its input.
    ///
    /// This represents a component we inject into the component topology that we send inputs to,
    /// which forwards them to the component being validated.
    Controlled,
}

impl RunnerInput {
    /// Consumes this runner input, providing the channel sender for sending input events to the
    /// component under validation.
    ///
    /// # Panics
    ///
    /// If the runner input is configured for an external resource, and a controlled edge is given,
    /// or if the runner input is configured for a controlled edge and no controlled edge is given,
    /// this function will panic, as one or the other must be provided.
    pub fn into_sender(self, controlled_edge: Option<mpsc::Sender<Event>>) -> mpsc::Sender<Event> {
        match (self, controlled_edge) {
            (Self::External(_), Some(_)) => panic!("Runner input declared as external resource, but controlled input edge was also specified."),
            (Self::Controlled, None) => panic!("Runner input declared as controlled, but no controlled input edge was specified."),
            (Self::External(tx), None) => tx,
            (Self::Controlled, Some(tx)) => tx,
        }
    }
}

/// Runner output mechanism.
///
/// This is the mechanism by which the runner task captures output from the component being
/// validated.
pub enum RunnerOutput {
    /// The component uses an external resource for its output.
    ///
    /// The channel provides a mechanism to send outputs collected by the external resource to the
    /// validation runner, whether the sink pushes output events to the external resource, or the
    /// external resource pulls output events from the sink.
    ///
    /// Only sinks have external inputs.
    External(mpsc::Receiver<Event>),

    /// The component uses a "controlled" edge for its output.
    ///
    /// This represents a component we inject into the component topology that we send outputs to,
    /// which forwards them to the validation runner.
    Controlled,
}

impl RunnerOutput {
    /// Consumes this runner output, providing the channel receiver for receiving output events from the
    /// component under validation.
    ///
    /// # Panics
    ///
    /// If the runner output is configured for an external resource, and a controlled edge is given,
    /// or if the runner output is configured for a controlled edge and no controlled edge is given,
    /// this function will panic, as one or the other must be provided.
    pub fn into_receiver(
        self,
        controlled_edge: Option<mpsc::Receiver<Event>>,
    ) -> mpsc::Receiver<Event> {
        match (self, controlled_edge) {
            (Self::External(_), Some(_)) => panic!("Runner output declared as external resource, but controlled output edge was also specified."),
            (Self::Controlled, None) => panic!("Runner output declared as controlled, but no controlled output edge was specified."),
            (Self::External(rx), None) => rx,
            (Self::Controlled, Some(rx)) => rx,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RunnerState {
    Running,
    InputDone,
    WaitingOnComponent,
    WaitingOnOutputs,
    Completed,
}

impl RunnerState {
    fn is_running(self) -> bool {
        self == RunnerState::Running
    }

    fn is_input_done(self) -> bool {
        self == RunnerState::InputDone
    }

    const fn is_component_active(self) -> bool {
        matches!(
            self,
            Self::Running | Self::InputDone | Self::WaitingOnComponent
        )
    }

    fn is_completed(self) -> bool {
        self == RunnerState::Completed
    }
}

pub struct RunnerResults {
    expectation: TestCaseExpectation,
    inputs: Vec<Event>,
    outputs: Vec<Event>,
    validator_results: Vec<Result<Vec<String>, Vec<String>>>,
}

impl RunnerResults {
    pub fn expectation(&self) -> TestCaseExpectation {
        self.expectation
    }

    pub fn inputs(&self) -> &[Event] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[Event] {
        &self.outputs
    }

    pub fn validator_results(&self) -> &[Result<Vec<String>, Vec<String>>] {
        &self.validator_results
    }
}

pub struct Runner<'comp, C: ?Sized> {
    validators: HashMap<String, Box<dyn Validator>>,
    component: &'comp C,
}

impl<'comp, C: ValidatableComponent + ?Sized> Runner<'comp, C> {
    pub fn from_component(component: &'comp C) -> Self {
        Self {
            component,
            validators: HashMap::new(),
        }
    }

    /// Adds a validator to this runner.
    ///
    /// ## Panics
    ///
    /// If another validator of the same type has already been added, this method will panic.
    pub fn add_validator<V>(&mut self, validator: V)
    where
        V: Into<Box<dyn Validator>>,
    {
        let validator = validator.into();
        let validator_name = validator.name();
        if self
            .validators
            .insert(validator_name.to_string(), validator)
            .is_some()
        {
            panic!(
                "attempted to add duplicate validator '{}' to runner",
                validator_name
            );
        }
    }

    pub async fn run_validation(self) -> Result<Vec<RunnerResults>, String> {
        // TODO: Make sure we initialize the metrics stuff in test mode so that it collects on a
        // per-thread basis, which we need to happen to avoid cross-contamination between tests.

        let mut test_case_results = Vec::new();

        let test_cases = self.component.test_cases();
        for test_case in test_cases {
            let (task_coordinator, task_shutdown_handle) = TaskCoordinator::new();

            // First, we get a topology builder for the given component being validated.
            //
            // The topology builder handles generating a valid topology (via `ConfigBuilder`) that
            // wires up the component being validated, as well as any filler components (i.e.
            // providing a source if the component being validated is a sink, and wiring the sink up
            // to that source, etc).
            //
            // We then finalize the topology builder to get our actual `ConfigBuilder`, as well as
            // any controlled edges (channel sender/receiver to the aforementioned filler
            // components) and a telemetry client for collecting internal telemetry.
            let topology_builder = TopologyBuilder::from_component_configuration(
                self.component.component_configuration(),
            );
            let (config_builder, controlled_edges, telemetry_collector) =
                topology_builder.finalize(&task_coordinator);

            // After that, we'll build the external resource necessary for this component, if any.
            // Once that's done, we build the input event/output event sender and receiver based on
            // whatever we spawned for an external resource.
            //
            // This handles spawning any intermediate tasks necessary, both for the external
            // resource itself, but also for the controlled edges.
            //
            // For example, if we're validating a source, we would have added a filler sink for our
            // controlled output edge, which means we then need a server task listening for the
            // events sent by that sink.
            let (runner_input, runner_output) =
                build_external_resource(self.component, &task_coordinator, task_shutdown_handle);
            let input_tx = runner_input.into_sender(controlled_edges.input);
            let mut output_rx = runner_output.into_receiver(controlled_edges.output);

            // Now with any external resource spawned, as well as any tasks for handling controlled
            // edges, we'll wait for all of those tasks to report that they're ready to go and
            // listening, etc.
            let mut task_coordinator = task_coordinator.wait_for_tasks_to_start().await;

            // At this point, we need to actually spawn the configured component topology so that it
            // runs, and make sure we have a way to tell it when to shutdown so that we can properly
            // sequence the test in terms of sending inputs, waiting for outputs, etc.
            let mut config = config_builder
                .build()
                .expect("config should not have any errors");
            config.healthchecks.set_require_healthy(Some(true));
            let config_diff = ConfigDiff::initial(&config);

            let (topology_task_coordinator, mut topology_shutdown_handle) = TaskCoordinator::new();
            let topology_started = topology_task_coordinator.track_started();
            let topology_completed = topology_task_coordinator.track_completed();

            let _test_runtime_thread = std::thread::spawn(move || {
                let test_runtime = Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("should not fail to build current-thread runtime");

                test_runtime.block_on(async move {
                    let pieces =
                        topology::build_or_log_errors(&config, &config_diff, HashMap::new())
                            .await
                            .unwrap();
                    let (topology, mut crash_rx) =
                        topology::start_validated(config, config_diff, pieces)
                            .await
                            .unwrap();

                    topology_started.mark_as_done();

                    select! {
                        // We got the signal to shutdown, so stop the topology gracefully.
                        _ = topology_shutdown_handle.wait() => {
                            topology.stop().await;
                            info!("Component topology stopped gracefully.")
                        },
                        _ = crash_rx.recv() => {
                            error!("Component topology under validation unexpectedly crashed.");
                        }
                    }

                    topology_completed.mark_as_done();
                });
            });

            let mut topology_task_coordinator =
                topology_task_coordinator.wait_for_tasks_to_start().await;

            // Now we'll spawn two tasks: one for sending inputs, and one for collecting outputs.
            //
            // We spawn these as discrete tasks because we want/need them to be able to run in an
            // interleaved fashion, and we want to wait for each of them to complete without the act
            // of waiting, in and of itself, causing any sort of deadlock behavior (i.e. trying to
            // send all inputs until we have no more, when we can't send more because we need to
            // drive output collection to allow forward progress to be made, etc.)

            // We sleep for one second here because while we do wait for the component topology to
            // mark itself as started, starting the topology does not necessaryily mean that all
            // component tasks are actually ready for input, etc.
            //
            // TODO: The above problem is bigger than just component validation, and affects a lot
            // of our unit tests that deal with spawning a topology and wanting to deterministically
            // know when it's safe to send inputs, etc... so we won't fix it here, but we should,
            // like the aforementioned unit tests, switch to any improved mechanism we come up with
            // in the future to make these tests more deterministic and waste less time waiting
            // around if we can avoid it.
            tokio::time::sleep(Duration::from_secs(1)).await;

            let input_events = test_case.events.clone();
            let input_driver = tokio::spawn(async move {
                for input_event in input_events {
                    input_tx
                        .send(input_event)
                        .await
                        .expect("input channel should not be closed");
                }
            });

            let output_driver = tokio::spawn(async move {
                let mut output_events = Vec::new();
                while let Some(output_event) = output_rx.recv().await {
                    output_events.push(output_event);
                }
                output_events
            });

            // Once we've sent all of the events, we'll drop our input sender which triggers the input
            // side -- external or controlled -- to finish whatever it's doing and gracefully close.
            // We'll wait for the input driver to complete, which implies it has sent all input
            // events and dropped the input sender.
            //
            // TODO: We need to wait here for our input-related tasks to mark themselves as
            // completed, but we currently mix/match all coordinated tasks together via
            // `task_coordinator` so we might need to tease that apart, as otherwise, we have no way
            // to wait for _only_ the input-related tasks.
            let _ = input_driver
                .await
                .expect("input driver task should not have panicked");

            // Now that the input side has marked itself as completed, we'll signal the component
            // topology to shutdown and wait until that happens.
            topology_task_coordinator
                .trigger_and_wait_for_shutdown()
                .await;

            // Now we'll trigger the output side, and telemetry collector, to finish up and
            // shutdown, and wait for that to happen.
            task_coordinator.trigger_and_wait_for_shutdown().await;
            let output_events = output_driver
                .await
                .expect("input driver task should not have panicked");

            // Now that the validation run has completed, run the results through each configured
            // validator, collect _their_ results, and store them to the side so we can run any
            // remaining test cases.
            let component_type = self.component.component_type();
            let expectation = test_case.expectation;
            let input_events = test_case.events;
            let telemetry_events = telemetry_collector.collect().await;

            let validator_results = self
                .validators
                .iter()
                .map(|(_, validator)| {
                    validator.check_validation(
                        component_type,
                        expectation,
                        &input_events,
                        &output_events,
                        &telemetry_events,
                    )
                })
                .collect();

            let test_case_result = RunnerResults {
                expectation,
                inputs: input_events,
                outputs: output_events,
                validator_results,
            };

            test_case_results.push(test_case_result);
        }

        Ok(test_case_results)
    }
}

fn build_external_resource<C: ValidatableComponent + ?Sized>(
    component: &C,
    task_coordinator: &TaskCoordinator<Configuring>,
    task_shutdown_handle: WaitHandle,
) -> (RunnerInput, RunnerOutput) {
    let component_type = component.component_type();
    let maybe_external_resource = component.external_resource();
    match component_type {
        ComponentType::Source => {
            // As an external resource for a source, we create a channel that the validation runner
            // uses to send the input events to the external resource. We don't care if the source
            // pulls those input events or has them pushed in: we just care about getting them to
            // the external resource.
            let (tx, rx) = mpsc::channel(1024);
            let resource =
                maybe_external_resource.expect("a source must always have an external resource");
            resource.spawn_as_input(rx, task_coordinator, task_shutdown_handle);

            (RunnerInput::External(tx), RunnerOutput::Controlled)
        }
        ComponentType::Transform => {
            // Transforms have no external resources.
            (RunnerInput::Controlled, RunnerOutput::Controlled)
        }
        ComponentType::Sink => {
            // As an external resource for a sink, we create a channel that the validation runner
            // uses to collect the output events from the external resource. We don't care if the sink
            // pushes those output events to the external resource, or if the external resource
            // pulls them from the sink: we just care about getting them from the external resource.
            let (tx, rx) = mpsc::channel(1024);
            let resource =
                maybe_external_resource.expect("a sink must always have an external resource");
            resource.spawn_as_output(tx, task_coordinator, task_shutdown_handle);

            (RunnerInput::Controlled, RunnerOutput::External(rx))
        }
    }
}
