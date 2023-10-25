use std::sync::Arc;

use tokio::sync::oneshot::{channel, Receiver};
use vector_lib::config::ComponentKey;
use vector_lib::config::OutputId;
use vector_lib::event::{Event, EventArray, EventContainer, LogEvent};

use crate::config::schema::Definition;
use crate::{
    config::{unit_test::UnitTestSourceConfig, ConfigBuilder},
    test_util::{
        components::assert_transform_compliance,
        mock::{
            oneshot_sink,
            transforms::{NoopTransformConfig, TransformType},
        },
        start_topology,
    },
    topology::RunningTopology,
};

async fn create_topology(
    event: Event,
    transform_type: TransformType,
) -> (RunningTopology, Receiver<EventArray>) {
    let mut builder = ConfigBuilder::default();

    let (tx, rx) = channel();

    builder.add_source(
        "in",
        UnitTestSourceConfig {
            events: vec![event],
        },
    );
    builder.add_transform(
        "transform",
        &["in"],
        NoopTransformConfig::from(transform_type),
    );
    builder.add_sink("out", &["transform"], oneshot_sink(tx));

    let config = builder.build().expect("building config should not fail");
    let (topology, _) = start_topology(config, false).await;

    (topology, rx)
}

#[tokio::test]
async fn test_function_transform_single_event() {
    assert_transform_compliance(async {
        let mut original_event = Event::Log(LogEvent::from("function transform being tested"));

        let (topology, rx) = create_topology(original_event.clone(), TransformType::Function).await;
        topology.stop().await;

        let events = rx.await.expect("must get back event from rx");
        let mut events = events.into_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);

        original_event.set_source_id(Arc::new(ComponentKey::from("in")));
        original_event.set_upstream_id(Arc::new(OutputId::from("transform")));
        original_event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        let event = events.remove(0);
        assert_eq!(original_event, event);
    })
    .await;
}

#[tokio::test]
async fn test_sync_transform_single_event() {
    assert_transform_compliance(async {
        let mut original_event = Event::Log(LogEvent::from("function transform being tested"));

        let (topology, rx) =
            create_topology(original_event.clone(), TransformType::Synchronous).await;
        topology.stop().await;

        let events = rx.await.expect("must get back event from rx");
        let mut events = events.into_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);

        original_event.set_source_id(Arc::new(ComponentKey::from("in")));
        original_event.set_upstream_id(Arc::new(OutputId::from("transform")));
        original_event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        let event = events.remove(0);
        assert_eq!(original_event, event);
    })
    .await;
}

#[tokio::test]
async fn test_task_transform_single_event() {
    assert_transform_compliance(async {
        let mut original_event = Event::Log(LogEvent::from("function transform being tested"));

        let (topology, rx) = create_topology(original_event.clone(), TransformType::Task).await;
        topology.stop().await;

        let events = rx.await.expect("must get back event from rx");
        let mut events = events.into_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);

        original_event.set_source_id(Arc::new(ComponentKey::from("in")));
        original_event.set_upstream_id(Arc::new(OutputId::from("transform")));
        original_event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        let event = events.remove(0);
        assert_eq!(original_event, event);
    })
    .await;
}
