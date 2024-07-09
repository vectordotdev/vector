pub mod config;
mod io;
mod telemetry;

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use bytes::BytesMut;
use chrono::Utc;
use tokio::{
    runtime::Builder,
    select,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
    task::JoinHandle,
};
use tokio_util::codec::Encoder as _;

use vector_lib::{
    codecs::encoding, config::LogNamespace, event::Event, EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::Encoder,
    components::validation::{RunnerMetrics, TestCase},
    config::ConfigBuilder,
    extra_context::ExtraContext,
    topology::RunningTopology,
};

use super::{
    encode_test_event,
    sync::{Configuring, TaskCoordinator},
    ComponentType, TestCaseExpectation, TestEvent, ValidationConfiguration, Validator,
};

pub use self::config::TopologyBuilder;

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
    External(mpsc::Receiver<Vec<Event>>),

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
        controlled_edge: Option<mpsc::Receiver<Vec<Event>>>,
    ) -> mpsc::Receiver<Vec<Event>> {
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
    extra_context: ExtraContext,
}

impl Runner {
    pub fn from_configuration(
        configuration: ValidationConfiguration,
        test_case_data_path: PathBuf,
        extra_context: ExtraContext,
    ) -> Self {
        Self {
            configuration,
            test_case_data_path,
            validators: HashMap::new(),
            extra_context,
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

    pub async fn run_validation(self) -> Result<Vec<RunnerResults>, vector_lib::Error> {
        // Initialize our test environment.
        initialize_test_environment();

        let mut test_case_results = Vec::new();

        let component_type = self.configuration.component_type();

        let test_cases = load_component_test_cases(&self.test_case_data_path)?;
        for test_case in test_cases {
            println!("");
            println!("");
            info!(
                "Running test '{}' case for component '{}' (type: {:?})...",
                test_case.name,
                self.configuration.component_name,
                self.configuration.component_type()
            );
            // Create a task coordinator for each relevant phase of the test.
            //
            // This provides us the granularity to know when the tasks associated with each phase
            // (inputs, component topology, outputs/telemetry, etc) have started, and the ability to
            // trigger them to shutdown and then wait until the associated tasks have completed.
            let input_task_coordinator = TaskCoordinator::new("Input");
            let output_task_coordinator = TaskCoordinator::new("Output");
            let topology_task_coordinator = TaskCoordinator::new("Topology");
            let telemetry_task_coordinator = TaskCoordinator::new("Telemetry");

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
            let topology_builder = TopologyBuilder::from_configuration(
                &self.configuration,
                test_case.config_name.as_ref(),
            )?;
            let (config_builder, controlled_edges, telemetry_collector) = topology_builder
                .finalize(
                    &input_task_coordinator,
                    &output_task_coordinator,
                    &telemetry_task_coordinator,
                )
                .await;

            info!("Component topology configuration built and telemetry collector spawned.");

            // Create the data structure that the input and output runners will use to store
            // their received/sent metrics. This is then shared with the Validator for comparison
            // against the actual metrics output by the component under test.
            let runner_metrics = Arc::new(Mutex::new(RunnerMetrics::default()));

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

            let (runner_input, runner_output, maybe_runner_encoder) = build_external_resource(
                &test_case,
                &self.configuration,
                &input_task_coordinator,
                &output_task_coordinator,
                &runner_metrics,
            )?;
            let input_tx = runner_input.into_sender(controlled_edges.input);
            let output_rx = runner_output.into_receiver(controlled_edges.output);
            info!("External resource (if any) and controlled edges built and spawned.");

            // Now with any external resource spawned, as well as any tasks for handling controlled
            // edges, we'll wait for all of those tasks to report that they're ready to go and
            // listening, etc.
            let mut input_task_coordinator = input_task_coordinator.started().await;
            info!("All input task(s) started.");

            let mut telemetry_task_coordinator = telemetry_task_coordinator.started().await;
            info!("All telemetry task(s) started.");

            let mut output_task_coordinator = output_task_coordinator.started().await;
            info!("All output task(s) started.");

            // At this point, we need to actually spawn the configured component topology so that it
            // runs, and make sure we have a way to tell it when to shutdown so that we can properly
            // sequence the test in terms of sending inputs, waiting for outputs, etc.
            spawn_component_topology(
                config_builder,
                &topology_task_coordinator,
                self.extra_context.clone(),
            );
            let mut topology_task_coordinator = topology_task_coordinator.started().await;

            // Now we'll spawn two tasks: one for sending inputs, and one for collecting outputs.
            //
            // We spawn these as discrete tasks because we want/need them to be able to run in an
            // interleaved fashion, and we want to wait for each of them to complete without the act
            // of waiting, in and of itself, causing any sort of deadlock behavior (i.e. trying to
            // send all inputs until we have no more, when we can't send more because we need to
            // drive output collection to allow forward progress to be made, etc.)

            // We sleep for two seconds here because while we do wait for the component topology to
            // mark itself as started, starting the topology does not necessarily mean that all
            // component tasks are actually ready for input, etc.
            //
            // TODO: The above problem is bigger than just component validation, and affects a lot
            // of our unit tests that deal with spawning a topology and wanting to deterministically
            // know when it's safe to send inputs, etc... so we won't fix it here, but we should,
            // like the aforementioned unit tests, switch to any improved mechanism we come up with
            // in the future to make these tests more deterministic and waste less time waiting
            // around if we can avoid it.
            tokio::time::sleep(Duration::from_secs(2)).await;

            let input_driver = spawn_input_driver(
                test_case.events.clone(),
                input_tx,
                &runner_metrics,
                maybe_runner_encoder.as_ref().cloned(),
                self.configuration.component_type,
                self.configuration.log_namespace(),
            );

            // the number of events we expect to receive from the output.
            let expected_output_events = test_case
                .events
                .iter()
                .filter(|te| !te.should_fail())
                .count();

            let output_driver = spawn_output_driver(
                output_rx,
                &runner_metrics,
                maybe_runner_encoder.as_ref().cloned(),
                self.configuration.component_type,
                expected_output_events,
            );

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

            // Synchronize the shutdown of all tasks, and get the resulting output events.
            // We drive the shutdown by ensuring that the output events have been
            // processed by the external resource, which ensures that the input events have travelled
            // all the way through the pipeline, and that the telemetry events have been processed
            // before shutting down the telemetry and topology tasks.
            input_task_coordinator.shutdown().await;

            let output_events = output_driver
                .await
                .expect("output driver task should not have panicked");

            // Now that all output events have been received, we can shutdown the controlled edge/sink
            output_task_coordinator.shutdown().await;

            // as well as the telemetry and topology
            telemetry_task_coordinator.shutdown().await;
            topology_task_coordinator.shutdown().await;

            info!("Collected runner metrics: {:?}", runner_metrics);
            let final_runner_metrics = runner_metrics.lock().await;

            // Run the relevant data -- inputs, outputs, telemetry, etc -- through each validator to
            // get the validation results for this test.
            let TestCase {
                name: test_name,
                expectation,
                events: input_events,
                ..
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
                        &final_runner_metrics,
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
fn load_component_test_cases(test_case_data_path: &PathBuf) -> Result<Vec<TestCase>, String> {
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
    test_case: &TestCase,
    configuration: &ValidationConfiguration,
    input_task_coordinator: &TaskCoordinator<Configuring>,
    output_task_coordinator: &TaskCoordinator<Configuring>,
    runner_metrics: &Arc<Mutex<RunnerMetrics>>,
) -> Result<(RunnerInput, RunnerOutput, Option<Encoder<encoding::Framer>>), vector_lib::Error> {
    let component_type = configuration.component_type();
    let maybe_external_resource = configuration.external_resource(test_case.config_name.as_ref());

    let resource_codec = maybe_external_resource
        .as_ref()
        .map(|resource| resource.codec.clone());

    let maybe_encoder = resource_codec.as_ref().map(|codec| codec.into_encoder());

    match component_type {
        ComponentType::Source => {
            // As an external resource for a source, we create a channel that the validation runner
            // uses to send the input events to the external resource. We don't care if the source
            // pulls those input events or has them pushed in: we just care about getting them to
            // the external resource.
            let (tx, rx) = mpsc::channel(1024);
            let resource =
                maybe_external_resource.expect("a source must always have an external resource");
            resource.spawn_as_input(rx, input_task_coordinator, runner_metrics);

            Ok((
                RunnerInput::External(tx),
                RunnerOutput::Controlled,
                maybe_encoder,
            ))
        }
        ComponentType::Transform => {
            // Transforms have no external resources.
            Ok((RunnerInput::Controlled, RunnerOutput::Controlled, None))
        }
        ComponentType::Sink => {
            // As an external resource for a sink, we create a channel that the validation runner
            // uses to collect the output events from the external resource. We don't care if the sink
            // pushes those output events to the external resource, or if the external resource
            // pulls them from the sink: we just care about getting them from the external resource.
            let (tx, rx) = mpsc::channel(1024);
            let resource =
                maybe_external_resource.expect("a sink must always have an external resource");

            resource.spawn_as_output(
                tx,
                output_task_coordinator,
                test_case.events.clone(),
                runner_metrics,
                configuration.log_namespace(),
            )?;

            Ok((
                RunnerInput::Controlled,
                RunnerOutput::External(rx),
                maybe_encoder,
            ))
        }
    }
}

fn spawn_component_topology(
    config_builder: ConfigBuilder,
    topology_task_coordinator: &TaskCoordinator<Configuring>,
    extra_context: ExtraContext,
) {
    let topology_started = topology_task_coordinator.track_started();
    let topology_completed = topology_task_coordinator.track_completed();
    let mut topology_shutdown_handle = topology_task_coordinator.register_for_shutdown();

    let mut config = config_builder
        .build()
        .expect("config should not have any errors");

    // It's possible we could extend the framework to allow specifying logic to
    // handle that, but I don't see much value currently since the healthcheck is
    // not enforced for components, and it doesn't impact the internal telemetry.
    config.healthchecks.enabled = false;

    _ = std::thread::spawn(move || {
        let test_runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build current-thread runtime");

        test_runtime.block_on(async move {
            info!("Building component topology...");

            let (topology, mut crash_rx) =
                RunningTopology::start_init_validated(config, extra_context)
                    .await
                    .unwrap();

            info!("Component topology built and spawned.");
            topology_started.mark_as_done();

            select! {
                // We got the signal to shutdown, so stop the topology gracefully.
                _ = topology_shutdown_handle.wait() => {
                    info!("Shutdown signal received, stopping topology...");
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
}

fn spawn_input_driver(
    input_events: Vec<TestEvent>,
    input_tx: Sender<TestEvent>,
    runner_metrics: &Arc<Mutex<RunnerMetrics>>,
    mut maybe_encoder: Option<Encoder<encoding::Framer>>,
    component_type: ComponentType,
    log_namespace: LogNamespace,
) -> JoinHandle<()> {
    let input_runner_metrics = Arc::clone(runner_metrics);

    let now = Utc::now();

    tokio::spawn(async move {
        for mut input_event in input_events {
            input_tx
                .send(input_event.clone())
                .await
                .expect("input channel should not be closed");

            // Update the runner metrics for the sent event. This will later
            // be used in the Validators, as the "expected" case.
            let mut input_runner_metrics = input_runner_metrics.lock().await;

            // the controlled edge (vector source) adds metadata to the event when it is received.
            // thus we need to add it here so the expected values for the comparisons on transforms
            // and sinks are accurate.
            if component_type != ComponentType::Source {
                if let Event::Log(ref mut log) = input_event.get_event() {
                    log_namespace.insert_standard_vector_source_metadata(log, "vector", now);
                }
            }

            let (failure_case, mut event) = input_event.clone().get();

            if let Some(encoder) = maybe_encoder.as_mut() {
                let mut buffer = BytesMut::new();
                encode_test_event(encoder, &mut buffer, input_event);

                input_runner_metrics.sent_bytes_total += buffer.len() as u64;
            }

            // account for failure case
            if failure_case {
                input_runner_metrics.errors_total += 1;
                // TODO: this assumption may need to be made configurable at some point
                if component_type == ComponentType::Sink {
                    input_runner_metrics.discarded_events_total += 1;
                }
            }

            if !failure_case || component_type == ComponentType::Sink {
                input_runner_metrics.sent_events_total += 1;

                // Convert unix timestamp in input events to the Datetime string.
                // This is necessary when a source expects the incoming event to have a
                // unix timestamp but we convert it into a datetime string in the source.
                // For example, the `datadog_agent` source. This only takes effect when
                // the test case YAML file defining the event, constructs it with the log
                // builder variant, and specifies an integer in milliseconds for the timestamp.
                if component_type == ComponentType::Source {
                    if let Event::Log(ref mut log) = event {
                        if let Some(ts) = log.remove_timestamp() {
                            let ts = match ts.as_integer() {
                                Some(ts) => chrono::DateTime::from_timestamp_millis(ts)
                                    .expect(&format!("invalid timestamp in input test event {ts}"))
                                    .into(),
                                None => ts,
                            };
                            log.parse_path_and_insert("timestamp", ts)
                                .expect("failed to insert timestamp");
                        }
                    }
                }

                // This particular metric is tricky because a component can run the
                // EstimatedJsonSizeOf calculation on a single event or an array of
                // events. If it's an array of events, the size calculation includes
                // the size of bracket ('[', ']') characters... But we have no way
                // of knowing which case it will be. Indeed, there are even components
                // where BOTH scenarios are possible, depending on how the component
                // is configured.
                // This is handled in the component spec validator code where we compare
                // the actual to the expected.
                input_runner_metrics.sent_event_bytes_total +=
                    event.estimated_json_encoded_size_of().get() as u64;
            }
        }
        info!("Input driver sent all events.");
    })
}

fn spawn_output_driver(
    mut output_rx: Receiver<Vec<Event>>,
    runner_metrics: &Arc<Mutex<RunnerMetrics>>,
    maybe_encoder: Option<Encoder<encoding::Framer>>,
    component_type: ComponentType,
    expected_events: usize,
) -> JoinHandle<Vec<Event>> {
    let output_runner_metrics = Arc::clone(runner_metrics);

    tokio::spawn(async move {
        let timeout = tokio::time::sleep(Duration::from_secs(8));
        tokio::pin!(timeout);

        let mut output_events = Vec::new();

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    error!("Output driver timed out waiting for all events.");
                    break
                },
                events = output_rx.recv() => {
                    if let Some(events) = events {
                        info!("Output driver received {} events.", events.len());
                        output_events.extend(events.clone());

                        // Update the runner metrics for the received event. This will later
                        // be used in the Validators, as the "expected" case.
                        let mut output_runner_metrics = output_runner_metrics.lock().await;

                        if component_type != ComponentType::Sink {
                            for output_event in events {
                                // The event is wrapped in a Vec to match the actual event storage in
                                // the real topology
                                output_runner_metrics.received_event_bytes_total +=
                                    vec![&output_event].estimated_json_encoded_size_of().get() as u64;

                                if let Some(encoder) = maybe_encoder.as_ref() {
                                    let mut buffer = BytesMut::new();
                                    encoder
                                        .clone()
                                        .encode(output_event, &mut buffer)
                                        .expect("should not fail to encode output event");

                                    output_runner_metrics.received_events_total += 1;
                                    output_runner_metrics.received_bytes_total += buffer.len() as u64;
                                }
                            }
                        }
                        if output_events.len() >= expected_events {
                            info!("Output driver has received all expected events.");
                            break
                        }
                    } else {
                        // The channel closed on us.
                        // This shouldn't happen because in the runner we should not shutdown the external
                        // resource until this output driver task is complete.
                        error!("Output driver channel with external resource closed.");
                        break
                    }
                }
            }
        }
        output_events
    })
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
