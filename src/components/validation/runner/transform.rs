use std::{future::Future, pin::Pin};

use tokio::sync::mpsc;
use vector_core::{
    event::{Event, EventContainer},
    transform::{FunctionTransform, OutputBuffer, Transform},
};

use crate::{
    components::validation::{
        sync::{Configured, ExternalResourceCoordinator},
        ComponentBuilderParts, ComponentType, ValidatableComponent,
    },
    config::TransformContext,
};

use super::{RunnerInput, RunnerOutput};

pub(super) async fn build_transform_component_future<C: ValidatableComponent>(
    component: C,
) -> (
    Pin<Box<dyn Future<Output = ()>>>,
    ExternalResourceCoordinator<Configured>,
    RunnerInput,
    RunnerOutput,
) {
    // As transforms have no external resources, we simply build the transform component and
    // wrap it so that we can drive it depending on which specific type of transform it is.
    let (input_tx, input_rx) = mpsc::channel(1024);
    let (output_tx, output_rx) = mpsc::channel(1024);
    let (resource_coordinator, _) =
        ExternalResourceCoordinator::from_component_type(ComponentType::Transform);

    let transform_context = TransformContext::default();
    let transform_builder_parts = ComponentBuilderParts::Transform(transform_context);
    let transform_component = component
        .build_component(transform_builder_parts)
        .await
        .expect("failed to build transform component")
        .into_transform_component();

    let fut = Box::pin(async move {
        match transform_component {
            Transform::Function(ft) => run_function_transform(ft, input_rx, output_tx).await,
            Transform::Synchronous(_st) => todo!(),
            Transform::Task(_tt) => todo!(),
        }

        debug!("Transform component completed.");
    });

    (fut, resource_coordinator, input_tx, output_rx)
}

async fn run_function_transform(
    mut transform: Box<dyn FunctionTransform>,
    mut input_rx: mpsc::Receiver<Event>,
    output_tx: mpsc::Sender<Event>,
) {
    while let Some(event) = input_rx.recv().await {
        let mut output_buf = OutputBuffer::default();
        transform.transform(&mut output_buf, event);

        for events in output_buf.take_events() {
            for event in events.into_events() {
                output_tx
                    .send(event)
                    .await
                    .expect("should not fail to send transformed event to output rx");
            }
        }
    }
}
