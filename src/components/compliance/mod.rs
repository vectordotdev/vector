mod resources;
mod payloads;

use std::{future::{Future, ready}, sync::Arc};

use tokio::sync::{oneshot, Barrier};

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

pub enum ResourcePayload {
	Json,
}

pub struct ExternalResource {
	direction: ResourceDirection,
	resource: ResourceDefinition,
	payload: ResourcePayload,
}

pub struct RunnerInput;

pub struct RunnerOutput;

pub enum ComponentInput {
	Channel,
	External,
}

pub enum ComponentOutput {
	Channel,
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
		self.shutdown_tx.send(()).expect("paired task should not yet be shutdown");
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

pub struct ComplianceRunner;

impl ComplianceRunner {
	fn build_component_task<C: Component>(shutdown_handle: ShutdownHandle, input: ComponentInput, output: ComponentOutput) -> impl Future<Output = ()> {
		ready(())
	}

	pub fn validate<C: Component>() -> Result<(), Vec<String>> {
		// Based on the component type, configure the necessary input/output.
		let (runner_input, component_input, component_output, runner_output) = match C::component_type() {
			ComponentType::Source => (RunnerInput, ComponentInput::External, ComponentOutput::Channel, RunnerOutput),
			ComponentType::Transform => (RunnerInput, ComponentInput::Channel, ComponentOutput::Channel, RunnerOutput),
			ComponentType::Sink => (RunnerInput, ComponentInput::Channel, ComponentOutput::External, RunnerOutput),
		};

		let (shutdown_trigger, shutdown_handle) = ShutdownTrigger::new();
		let component_task = Self::build_component_task::<C>(shutdown_handle, component_input, component_output);

		Ok(())
	}
}