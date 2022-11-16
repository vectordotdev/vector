mod sink;
mod source;
mod transform;

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    iter,
    pin::Pin,
    time::Duration,
};

use tokio::{pin, runtime::Builder, select, sync::mpsc};
use vector_common::finalization::{EventStatus, Finalizable};
use vector_core::event::{Event, LogEvent};

use crate::components::compliance::sync::ExternalResourceCoordinator;

use super::{
    sync::Configured, ComponentType, ValidatableComponent, Validator, WaitHandle, WaitTrigger,
};

use self::sink::build_sink_component_future;
use self::source::build_source_component_future;
use self::transform::build_transform_component_future;

/// Runner input mechanism.
///
/// This is the mechanism by which the Runner task pushes input to the component being validated.
pub type RunnerInput = mpsc::Sender<Event>;
/// Runner output mechanism.
///
/// This is the mechanism by which the Runner task captures output from the component being
/// validated.
pub type RunnerOutput = mpsc::Receiver<Event>;

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
    inputs: Vec<Event>,
    outputs: Vec<Event>,
    validator_results: Vec<Result<Vec<String>, Vec<String>>>,
}

impl RunnerResults {
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

// TODO: We might actually want to make the runner spin up its own current-thread runtime so that we
// can't shoot ourselves in the foot and run under a multi-threaded executor, since a lot of the
// validation will depend on the component future running on the same thread as we're collecting the
// validation results from i.e. metrics and so on.
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

    async fn build_component_future(
        &mut self,
        component_shutdown_handle: WaitHandle,
    ) -> (
        Pin<Box<dyn Future<Output = ()>>>,
        ExternalResourceCoordinator<Configured>,
        RunnerInput,
        RunnerOutput,
    ) {
        match self.component.component_type() {
            ComponentType::Source => {
                build_source_component_future(&self.component, component_shutdown_handle).await
            }
            ComponentType::Transform => build_transform_component_future(&self.component).await,
            ComponentType::Sink => build_sink_component_future(&self.component).await,
        }
    }

    fn generate_input_payloads(&self) -> VecDeque<Event> {
        let mut log_event = LogEvent::default();
        log_event.insert("message", "compliance");
        iter::once(Event::Log(log_event)).cycle().take(3).collect()
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

    pub fn run_validation(mut self) -> Result<RunnerResults, String> {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build current-thread runtime");

        runtime.block_on(async move {
            // Build our component future, which gets us the resulting input/output objects necessary to
            // drive it. This will also spawn any external resource necessary for the given component,
            // which also gets us the input/output objects necessary to send input payloads, or receive
            // output payloads.
            let (component_shutdown_trigger, component_shutdown_handle) = WaitTrigger::new();
            let (component_future, resource_coordinator, runner_input, mut runner_output) =
                self.build_component_future(component_shutdown_handle).await;

            // Wait for our external resource tasks, if any, to start.
            //
            // Once the external resource tasks are ready, start driving our core validation loop, where
            // we inject events, drive the component itself to run and process those events, and then
            // collect all results at the end.
            let mut resource_coordinator = resource_coordinator.wait_for_tasks_to_start().await;

            // Our core logic is straightforward: send inputs from the runner to the component, let
            // the component process them, and have the runner collect any outputs from the
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

            // Run the pre-run hooks for all of our validators, which lets them configure and hook into
            // various subsystems, as needed, to perform their respective validation tasks.
            for validator in &mut self.validators.values_mut() {
                validator.run_pre_hook(self.component.component_type(), &input_payloads_result[..]);
            }

            // Spawn a task that drives the sending of inputs to the component, whether it's
            // direct or happening through an external resource. We do this here mostly to avoid tricky
            // code in the core loop for dealing with cancellable futures and being able to drop the
            // input sender when we've sent all inputs.

            // TODO: We need a barrier here, to ensure that we don't start sending inputs to the
            // external resource/component _until_ the component is ready. This is essentially only a
            // problem for sources, where they start listening/polling for input data
            // nondeterministically, which is not great for testing scenarios.
            //
            // We'll probably need to approximate this in the meantime by not letting the input task
            // start sending messages until after like.. one or two seconds since the first time we
            // polled the (source) component, since most sources will only start interacting with their
            // input source once they've been polled for the first time.
            let mut input_task_handle = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;

                while let Some(input) = input_payloads.pop_front() {
                    runner_input
                        .send(input)
                        .await
                        .expect("should not fail to send runner input to component");
                }

                drop(runner_input);
            });

            let mut component_shutdown_trigger = Some(component_shutdown_trigger);
            pin!(component_future);

            let mut runner_state = RunnerState::Running;
            while !runner_state.is_completed() {
                let maybe_new_runner_state = select! {
                    // Drive our input sender until it's done sending all of the inputs.
                    result = &mut input_task_handle, if runner_state.is_running() => match result {
                        Ok(()) => Some(RunnerState::InputDone),
                        Err(_) => panic!("Runner input sender task panicked unexpectedly."),
                    },

                    // When all of the inputs are done being sent, we trigger any external input
                    // resource tasks to shutdown, finishing up their work and returning. Once that's
                    // complete, we can then trigger the component itself to shutdown.
                    _ = resource_coordinator.trigger_and_wait_for_input_shutdown(), if runner_state.is_input_done() => {
                        // Now signal the actual component to shutdown. Depending on the component,
                        // this might also be a no-op, since source have to be tirggered to
                        // shutdown, but transforms and sinks shut themselves down when the input
                        // stream ends, which will happen when our Runner input sender task
                        // finishes sending all input payloads.
                        component_shutdown_trigger.take().expect("component shutdown trigger already consumed").trigger();

                        Some(RunnerState::WaitingOnComponent)
                    },

                    // Drive the component if it is still actively processing.
                    _ = &mut component_future, if runner_state.is_component_active() => {
                        // When the component finishes, that means that all we have left to do is make
                        // sure we've collected all of our outputs. We'll send the shutdown signal to
                        // the external output resource, if it exists. This is to ensure that it flushes
                        // any of its own internal state, or does a final poll, or whatever it has to
                        // do, before we continue draining the Runner outputs.
                        //
                        // If we're not validating a sink component, these calls are no-ops that will
                        // complete immediately.
                        resource_coordinator.trigger_and_wait_for_output_shutdown().await;

                        // At this point, since the component is done, we're just waiting for the
                        // remaining outputs to be collected before we're all done.
                        Some(RunnerState::WaitingOnOutputs)
                    },

                    // We got an output from the component, so stash it off to the side for now.
                    maybe_output = runner_output.recv(), if !runner_state.is_completed() => match maybe_output {
                        Some(mut output) => {
                            // Finalize the event, which allows sources to shutdown cleanly if they're
                            // waiting for acknowledgement-related wakeups.
                            let mut finalizers = output.take_finalizers();
                            finalizers.update_status(EventStatus::Delivered);
                            finalizers.update_sources();

                            output_payloads.push(output);
                            None
                        },

                        // The channel can only be closed when the component has completed, so if we're here, mark
                        // ourselves as having completed.
                        None => Some(RunnerState::Completed),
                    },
                };

                if let Some(new_runner_state) = maybe_new_runner_state {
                    let existing_runner_state = runner_state;
                    runner_state = new_runner_state;

                    debug!(
                        "Runner state transitioned from {:?} to {:?}.",
                        existing_runner_state, new_runner_state
                    );
                }
            }

            // Run the post-run hooks for all of our validators, which is where they would do any final
            // cleanup, flushing, scraping of data, and so on. In doing so, we'll also collect their
            // validation results.
            let mut validator_results = Vec::new();
            for (_, mut validator) in self.validators.into_iter() {
                validator.run_post_hook(&output_payloads[..]);
                validator_results.push(validator.into_results());
            }

            Ok(RunnerResults {
                inputs: input_payloads_result,
                outputs: output_payloads,
                validator_results,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use vector_config::NamedComponent;
    use vector_core::{
        event::Event,
        transform::{FunctionTransform, OutputBuffer, Transform},
    };

    use crate::components::compliance::{BuiltComponent, ComponentBuilderParts, ExternalResource};

    use super::*;

    // A simple transform that just forwards its event untouched.
    #[derive(Clone)]
    struct ValidatableTransform;

    impl FunctionTransform for ValidatableTransform {
        fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
            output.push(event);
        }
    }

    impl NamedComponent for ValidatableTransform {
        const NAME: &'static str = "validatable_transform";
    }

    #[async_trait]
    impl ValidatableComponent for ValidatableTransform {
        fn component_name(&self) -> &'static str {
            Self::NAME
        }

        fn component_type(&self) -> ComponentType {
            ComponentType::Transform
        }

        fn external_resource(&self) -> Option<ExternalResource> {
            None
        }

        async fn build_component(
            &self,
            _builder_parts: ComponentBuilderParts,
        ) -> Result<BuiltComponent, String> {
            Ok(BuiltComponent::Transform(Transform::Function(Box::new(
                ValidatableTransform,
            ))))
        }
    }

    #[test]
    fn basic() {
        let component = ValidatableTransform;
        let runner = Runner::from_component(&component);
        let results = runner.run_validation();
        assert!(results.is_ok());

        let results = results.expect("results should be ok");
        assert_eq!(results.inputs, results.outputs);
    }
}
