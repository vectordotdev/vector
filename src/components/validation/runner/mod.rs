mod config;
mod io;
mod telemetry;

use std::{collections::HashMap, path::PathBuf, time::Duration};

use tokio::{runtime::Builder, select, sync::mpsc};
use vector_core::event::Event;

use crate::{
    components::validation::TestCase,
    config::{ConfigBuilder, ConfigDiff},
    topology,
};

use super::{
    sync::{Configuring, TaskCoordinator},
    ComponentType, TestCaseExpectation, TestEvent, ValidationConfiguration, Validator,
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
    External(mpsc::Sender<TestEvent>),

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
    pub fn into_sender(
        self,
        controlled_edge: Option<mpsc::Sender<TestEvent>>,
    ) -> mpsc::Sender<TestEvent> {
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

pub struct RunnerResults {
    test_name: String,
    expectation: TestCaseExpectation,
    inputs: Vec<TestEvent>,
    outputs: Vec<Event>,
    validator_results: Vec<Result<Vec<String>, Vec<String>>>,
}

impl RunnerResults {
    pub fn test_name(&self) -> &str {
        &self.test_name
    }

    pub const fn expectation(&self) -> TestCaseExpectation {
        self.expectation
    }

    pub fn inputs(&self) -> &[TestEvent] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[Event] {
        &self.outputs
    }

    pub fn validator_results(&self) -> &[Result<Vec<String>, Vec<String>>] {
        &self.validator_results
    }
}

pub struct Runner {
    configuration: ValidationConfiguration,
    test_case_data_path: PathBuf,
    validators: HashMap<String, Box<dyn Validator>>,
}

impl Runner {
    pub fn from_configuration(
        configuration: ValidationConfiguration,
        test_case_data_path: PathBuf,
    ) -> Self {
        Self {
            configuration,
            test_case_data_path,
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
        // Initialize our test environment.
        initialize_test_environment();

        let mut test_case_results = Vec::new();

        let component_type = self.configuration.component_type();

        let test_cases = load_component_test_cases(self.test_case_data_path)?;
        for test_case in test_cases {
            // Create a task coordinator for each relevant phase of the test.
            //
            // This provides us the granularity to know when the tasks associated with each phase
            // (inputs, component topology, outputs/telemetry, etc) have started, and the ability to
            // trigger them to shutdown and then wait until the associated tasks have completed.
            let input_task_coordinator = TaskCoordinator::new();
            let output_task_coordinator = TaskCoordinator::new();
            let topology_task_coordinator = TaskCoordinator::new();

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
            let topology_builder = TopologyBuilder::from_configuration(&self.configuration);
            let (config_builder, controlled_edges, telemetry_collector) =
                topology_builder.finalize(&input_task_coordinator, &output_task_coordinator);
            debug!("Component topology configuration built and telemetry collector spawned.");

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
            let (runner_input, runner_output) = build_external_resource(
                &self.configuration,
                &input_task_coordinator,
                &output_task_coordinator,
            );
            let input_tx = runner_input.into_sender(controlled_edges.input);
            let mut output_rx = runner_output.into_receiver(controlled_edges.output);
            debug!("External resource (if any) and controlled edges built and spawned.");

            // Now with any external resource spawned, as well as any tasks for handling controlled
            // edges, we'll wait for all of those tasks to report that they're ready to go and
            // listening, etc.
            let input_task_coordinator = input_task_coordinator.started().await;
            debug!("All input task(s) started.");

            let output_task_coordinator = output_task_coordinator.started().await;
            debug!("All output task(s) started.");

            // At this point, we need to actually spawn the configured component topology so that it
            // runs, and make sure we have a way to tell it when to shutdown so that we can properly
            // sequence the test in terms of sending inputs, waiting for outputs, etc.
            spawn_component_topology(config_builder, &topology_task_coordinator);
            let topology_task_coordinator = topology_task_coordinator.started().await;

            // Now we'll spawn two tasks: one for sending inputs, and one for collecting outputs.
            //
            // We spawn these as discrete tasks because we want/need them to be able to run in an
            // interleaved fashion, and we want to wait for each of them to complete without the act
            // of waiting, in and of itself, causing any sort of deadlock behavior (i.e. trying to
            // send all inputs until we have no more, when we can't send more because we need to
            // drive output collection to allow forward progress to be made, etc.)

            // We sleep for one second here because while we do wait for the component topology to
            // mark itself as started, starting the topology does not necessarily mean that all
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

            // At this point, the component topology is running, and all input/output/telemetry
            // tasks are running as well. Our input driver should be sending (or will have already
            // sent) all of the input events, which will cascade through the component topology as
            // they're processed.
            //
            // We'll trigger each phase to shutdown, in order, to deterministically ensure each
            // section has completed. We additionally wait for the input driver task to complete
            // first, and the output driver task to complete last, as those tasks are freerunning
            // and don't require special shutdown coordination.
            input_driver
                .await
                .expect("input driver task should not have panicked");

            input_task_coordinator.shutdown().await;
            debug!("Input task(s) have been shutdown.");

            topology_task_coordinator.shutdown().await;
            debug!("Component topology task has been shutdown.");

            output_task_coordinator.shutdown().await;
            debug!("Output task(s) have been shutdown.");

            let output_events = output_driver
                .await
                .expect("input driver task should not have panicked");

            // Run the relevant data -- inputs, outputs, telemetry, etc -- through each validator to
            // get the validation results for this test.
            let TestCase {
                name: test_name,
                expectation,
                events: input_events,
            } = test_case;
            let telemetry_events = telemetry_collector.collect().await;

            let validator_results = self
                .validators
                .values()
                .map(|validator| {
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
                test_name,
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

/// Loads all of the test cases for the given component.
///
/// Test cases are searched for in a file that must be located at
/// `tests/validation/components/<component type>/<component name>.yaml`, where the component type
/// is the type of the component (either `sources`, `transforms`, or `sinks`) and the component name
/// is the value given by `ValidatableComponent::component_name`.
///
/// As implied by the file path, the file is expected to be valid YAML, containing an array of test
/// cases.
///
/// ## Errors
///
/// If an I/O error is encountered during the loading of the test case file, or any error occurs
/// during deserialization of the test case file, whether the error is I/O related in nature or due
/// to invalid YAML, or not representing valid serialized test cases, then an error variant will be
/// returned explaining the cause.
fn load_component_test_cases(test_case_data_path: PathBuf) -> Result<Vec<TestCase>, String> {
    std::fs::File::open(test_case_data_path)
        .map_err(|e| {
            format!(
                "I/O error during open of component validation test cases file: {}",
                e
            )
        })
        .and_then(|file| {
            serde_yaml::from_reader(file).map_err(|e| {
                format!(
                    "Deserialization error for component validation test cases file: {}",
                    e
                )
            })
        })
}

fn build_external_resource(
    configuration: &ValidationConfiguration,
    input_task_coordinator: &TaskCoordinator<Configuring>,
    output_task_coordinator: &TaskCoordinator<Configuring>,
) -> (RunnerInput, RunnerOutput) {
    let component_type = configuration.component_type();
    let maybe_external_resource = configuration.external_resource();
    match component_type {
        ComponentType::Source => {
            // As an external resource for a source, we create a channel that the validation runner
            // uses to send the input events to the external resource. We don't care if the source
            // pulls those input events or has them pushed in: we just care about getting them to
            // the external resource.
            let (tx, rx) = mpsc::channel(1024);
            let resource =
                maybe_external_resource.expect("a source must always have an external resource");
            resource.spawn_as_input(rx, input_task_coordinator);

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
            resource.spawn_as_output(tx, output_task_coordinator);

            (RunnerInput::Controlled, RunnerOutput::External(rx))
        }
    }
}

fn spawn_component_topology(
    config_builder: ConfigBuilder,
    topology_task_coordinator: &TaskCoordinator<Configuring>,
) {
    let topology_started = topology_task_coordinator.track_started();
    let topology_completed = topology_task_coordinator.track_completed();
    let mut topology_shutdown_handle = topology_task_coordinator.register_for_shutdown();

    let mut config = config_builder
        .build()
        .expect("config should not have any errors");
    config.healthchecks.set_require_healthy(Some(true));
    let config_diff = ConfigDiff::initial(&config);

    let _ = std::thread::spawn(move || {
        let test_runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build current-thread runtime");

        test_runtime.block_on(async move {
            debug!("Building component topology...");

            let pieces = topology::build_or_log_errors(&config, &config_diff, HashMap::new())
                .await
                .unwrap();
            let (topology, (_, mut crash_rx)) =
                topology::start_validated(config, config_diff, pieces)
                    .await
                    .unwrap();

            debug!("Component topology built and spawned.");
            topology_started.mark_as_done();

            select! {
                // We got the signal to shutdown, so stop the topology gracefully.
                _ = topology_shutdown_handle.wait() => {
                    debug!("Shutdown signal received, stopping topology...");
                    topology.stop().await;
                    debug!("Component topology stopped gracefully.")
                },
                _ = crash_rx.recv() => {
                    error!("Component topology under validation unexpectedly crashed.");
                }
            }

            topology_completed.mark_as_done();
        });
    });
}

fn initialize_test_environment() {
    // Make sure our metrics recorder is installed and in test mode. This is necessary for
    // proper internal telemetry collect when running the component topology, even though it's
    // running in an isolated current-thread runtime, as test mode isolates metrics on a
    // per-thread basis.
    crate::metrics::init_test();

    // Make sure that early buffering is stopped for logging.
    //
    // If we don't do this, the `internal_logs` source can never actually run in a meaningful way,
    // which means it ends up deadlocked, unable to process input _or_ respond to a shutdown signal.
    crate::trace::stop_early_buffering();
}
