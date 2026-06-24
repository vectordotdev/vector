use tokio::{
    sync::oneshot,
    time::{Duration, timeout},
};
use vector_lib::{config::ComponentKey, metrics::Controller};

use crate::{
    config::Config,
    event::{Event, LogEvent, Metric, MetricValue},
    test_util::{
        mock::{
            basic_source,
            sinks::CompletionSinkConfig,
            transforms::{NoopTransformConfig, TransformType},
        },
        start_topology, trace_init,
    },
};

const EVENT_COUNT: usize = 100;
const SOURCE_ID: &str = "cpu_source";
const TRANSFORM_ID: &str = "cpu_transform";
const TRANSFORM_TYPE: &str = "test_noop";
const TRANSFORM_KIND: &str = "transform";
const SINK_ID: &str = "cpu_sink";

/// Builds and runs a source → transform (with `measure_cpu_usage = true`) → sink
/// topology, sends `EVENT_COUNT` events through it, and returns the captured
/// metrics after the topology stops.
///
/// `make_config` receives a base `NoopTransformConfig` and can apply extra
/// options (e.g. `.with_concurrency()`) before the topology is assembled.
async fn run_cpu_topology(
    make_config: impl FnOnce(NoopTransformConfig) -> NoopTransformConfig,
) -> Vec<Metric> {
    trace_init();

    let controller = Controller::get().expect("metrics controller");
    controller.reset();

    let (mut source_tx, source_config) = basic_source();
    let (sink_done_tx, sink_done_rx) = oneshot::channel();
    let sink_config = CompletionSinkConfig::new(EVENT_COUNT, sink_done_tx);

    let mut config = Config::builder();
    config.add_source(SOURCE_ID, source_config);

    // Add a plain noop transform first, then flip the measure_cpu_usage flag on
    // the TransformOuter that the builder just inserted.
    config.add_transform(
        TRANSFORM_ID,
        &[SOURCE_ID],
        make_config(NoopTransformConfig::from(TransformType::Function)),
    );
    config
        .transforms
        .get_mut(&ComponentKey::from(TRANSFORM_ID))
        .expect("transform not found in builder")
        .measure_cpu_usage = true;

    config.add_sink(SINK_ID, &[TRANSFORM_ID], sink_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    for idx in 0..EVENT_COUNT {
        let event = Event::Log(LogEvent::from(format!("payload-{idx}")));
        source_tx.send_event(event).await.unwrap();
    }

    drop(source_tx);

    let completed = timeout(Duration::from_secs(5), sink_done_rx)
        .await
        .expect("timed out waiting for completion sink to finish")
        .expect("completion sink sender dropped");
    assert!(
        completed,
        "completion sink finished before receiving all events"
    );

    topology.stop().await;

    controller.capture_metrics()
}

fn has_transform_tags(metric: &Metric) -> bool {
    metric.tags().is_some_and(|tags| {
        tags.get("component_id") == Some(TRANSFORM_ID)
            && tags.get("component_type") == Some(TRANSFORM_TYPE)
            && tags.get("component_kind") == Some(TRANSFORM_KIND)
    })
}

fn assert_cpu_counter_positive(metrics: &[Metric]) {
    let cpu_metric = metrics
        .iter()
        .find(|m| m.name() == "component_cpu_usage_ns_total" && has_transform_tags(m))
        .unwrap_or_else(|| {
            panic!(
                "component_cpu_usage_ns_total not found for transform '{}'; \
                 available metrics: {:?}",
                TRANSFORM_ID,
                metrics
                    .iter()
                    .map(|m| m.name())
                    .collect::<std::collections::BTreeSet<_>>(),
            )
        });

    match cpu_metric.value() {
        MetricValue::Counter { value } => {
            assert!(
                *value > 0.0,
                "expected component_cpu_usage_ns_total > 0, got {value}"
            );
        }
        other => panic!("expected Counter metric, got {other:?}"),
    }
}

/// Function transform (non-concurrent, inline runner): `component_cpu_usage_ns_total`
/// must be present with the correct component tags and a positive value when
/// `measure_cpu_usage = true`.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[tokio::test]
async fn component_cpu_usage_emitted_function_transform() {
    let metrics = run_cpu_topology(|c| c).await;
    assert_cpu_counter_positive(&metrics);
}

/// Task transform (non-concurrent): the same `cpu_timed` wrapper is applied to
/// the task future in `build_task_transform`, so the counter must also be
/// emitted when `measure_cpu_usage = true`.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[tokio::test]
async fn component_cpu_usage_emitted_task_transform() {
    let metrics = run_cpu_topology(|_| NoopTransformConfig::from(TransformType::Task)).await;
    assert_cpu_counter_positive(&metrics);
}

/// Concurrent sync transform: the driver future goes through `run_concurrently()`
/// in `build_sync_transform`, and each spawned batch task is wrapped via
/// `spawn_timed`. The counter must still be emitted when `measure_cpu_usage = true`.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[tokio::test]
async fn component_cpu_usage_emitted_concurrent_transform() {
    let metrics = run_cpu_topology(|c| c.with_concurrency()).await;
    assert_cpu_counter_positive(&metrics);
}

/// When `measure_cpu_usage = false` (the default), no
/// `component_cpu_usage_ns_total` counter should be emitted for the transform.
#[tokio::test]
async fn component_cpu_usage_not_emitted_without_measure_cpu_usage() {
    trace_init();

    let controller = Controller::get().expect("metrics controller");
    controller.reset();

    let (mut source_tx, source_config) = basic_source();
    let (sink_done_tx, sink_done_rx) = oneshot::channel();
    let sink_config = CompletionSinkConfig::new(EVENT_COUNT, sink_done_tx);

    let mut config = Config::builder();
    config.add_source(SOURCE_ID, source_config);
    // Default: measure_cpu_usage = false
    config.add_transform(
        TRANSFORM_ID,
        &[SOURCE_ID],
        NoopTransformConfig::from(TransformType::Function),
    );
    config.add_sink(SINK_ID, &[TRANSFORM_ID], sink_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    for idx in 0..EVENT_COUNT {
        let event = Event::Log(LogEvent::from(format!("payload-{idx}")));
        source_tx.send_event(event).await.unwrap();
    }
    drop(source_tx);

    timeout(Duration::from_secs(5), sink_done_rx)
        .await
        .expect("timed out")
        .expect("sender dropped");

    topology.stop().await;

    let metrics = controller.capture_metrics();
    let found = metrics
        .iter()
        .any(|m| m.name() == "component_cpu_usage_ns_total" && has_transform_tags(m));

    assert!(
        !found,
        "component_cpu_usage_ns_total should NOT be emitted when measure_cpu_usage = false"
    );
}
