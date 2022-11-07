use std::{future::Future, pin::Pin};

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vector_core::{
    config::{proxy::ProxyConfig, GlobalOptions},
    event::Event,
};

use crate::{
    components::compliance::{
        sync::{Configured, ExternalResourceCoordinator},
        ComponentBuilderParts, ComponentType, ValidatableComponent,
    },
    config::{schema, SinkContext, SinkHealthcheckOptions},
};

use super::{RunnerInput, RunnerOutput};

pub(super) async fn build_sink_component_future<C: ValidatableComponent>(
    component: C,
) -> (
    Pin<Box<dyn Future<Output = ()>>>,
    ExternalResourceCoordinator<Configured>,
    RunnerInput,
    RunnerOutput,
) {
    // First we'll spawn the external output resource. We ensure that the external resource is
    // ready via `tasks_started` when the validator actually runs.
    let (input_tx, input_rx) = mpsc::channel(1024);
    let (runner_tx, runner_rx) = mpsc::channel(1024);
    let (resource_coordinator, resource_shutdown_handle) =
        ExternalResourceCoordinator::from_component_type(ComponentType::Sink);
    let resource = component
        .external_resource()
        .expect("a sink must always have an external resource");
    resource.spawn_as_output(runner_tx, &resource_coordinator, resource_shutdown_handle);

    // Now actually build the sink itself. We end up wrapping it in a very thin layer of glue to
    // drive it properly and mark when the component completes.
    let sink_context = SinkContext {
        healthcheck: SinkHealthcheckOptions::default(),
        globals: GlobalOptions::default(),
        proxy: ProxyConfig::default(),
        schema: schema::Options::default(),
    };

    let sink_builder_parts = ComponentBuilderParts::Sink(sink_context);
    let sink_component = component
        .build_component(sink_builder_parts)
        .await
        .expect("failed to build sink component")
        .into_sink_component();

    let fut = Box::pin(async move {
        let input_stream = ReceiverStream::new(input_rx).map(|p: Event| p.into());

        if let Err(()) = sink_component.run(input_stream).await {
            panic!("sink completed with error");
        } else {
            debug!("Sink component completed without error.");
        }
    });

    (fut, resource_coordinator, input_tx, runner_rx)
}
