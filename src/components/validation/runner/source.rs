use std::{collections::HashMap, future::Future, pin::Pin};

use tokio::{pin, select, sync::mpsc};
use vector_common::{config::ComponentKey, shutdown::ShutdownSignal};
use vector_core::{
    config::{proxy::ProxyConfig, GlobalOptions},
    event::EventContainer,
};

use crate::{
    components::validation::{
        sync::{Configured, ExternalResourceCoordinator, WaitHandle},
        ComponentBuilderParts, ComponentType, ValidatableComponent,
    },
    config::{schema, SourceContext},
    SourceSender,
};

use super::{RunnerInput, RunnerOutput};

pub(super) async fn build_source_component_future<C: ValidatableComponent>(
    component: C,
    mut component_shutdown_handle: WaitHandle,
) -> (
    Pin<Box<dyn Future<Output = ()>>>,
    ExternalResourceCoordinator<Configured>,
    RunnerInput,
    RunnerOutput,
) {
    // First we'll spawn the external input resource. We ensure that the external resource is
    // ready via `tasks_started` when the validator actually runs.
    let (input_tx, input_rx) = mpsc::channel(1024);
    let (runner_tx, runner_rx) = mpsc::channel(1024);
    let (resource_coordinator, resource_shutdown_handle) =
        ExternalResourceCoordinator::from_component_type(ComponentType::Source);
    let resource = component
        .external_resource()
        .expect("a source must always have an external resource");
    resource.spawn_as_input(input_rx, &resource_coordinator, resource_shutdown_handle);

    // Now actually build the source itself. We end up wrapping it in a very thin layer of glue to
    // drive it properly and ensure that we trigger it to shutdown when the validator tells us
    // that it's time to shutdown from its perspective.
    let (source_tx, mut source_rx) = SourceSender::new_with_buffer(1024);
    let (source_shutdown_trigger, shutdown, _) = ShutdownSignal::new_wired();
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
    let source_component = component
        .build_component(source_builder_parts)
        .await
        .expect("failed to build source component")
        .into_source_component();

    let fut = Box::pin(async move {
        let mut source_shutdown_trigger = Some(source_shutdown_trigger);
        pin!(source_component);

        let mut source_done = false;

        loop {
            select! {
                // Wait for the shutdown signal from the validator, and then trigger
                // shutdown of the source with its native shutdown signal.
                _ = component_shutdown_handle.wait(), if source_shutdown_trigger.is_some() => {
                    debug!("Source received shutdown signal from runner, forwarding...");
                    drop(source_shutdown_trigger.take());
                },

                // Drive the source component until it completes, in which case we're done. This
                // should really only occur once we've triggered shutdown.
                result = &mut source_component, if !source_done => {
                    debug!("Source component completed.");

                    if source_shutdown_trigger.is_some() {
                        panic!("source component completed prior to shutdown being triggered");
                    }

                    if let Err(()) = result {
                        panic!("source completed with error");
                    } else {
                        debug!("Source component completed without error.");
                    }

                    source_done = true;
                },

                // If the source sent an output event, forward it to the runner.
                Some(events) = source_rx.next() => for event in events.into_events() {
                    runner_tx.send(event).await.expect(
                        "validator receiver should not be closed before the component completes",
                    );
                },

                else => {
                    debug!("Source component task complete.");
                    break
                },
            }
        }
    });

    (fut, resource_coordinator, input_tx, runner_rx)
}
