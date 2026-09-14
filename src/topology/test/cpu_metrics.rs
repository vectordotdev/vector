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
const TRANSFORM_TYPE: &str = "test_noop";
const TRANSFORM_KIND: &str = "transform";

/// Per-event busy-spin duration used by the CPU-usage tests below.
///
/// OS-level thread CPU-time clocks (e.g. Windows' `GetThreadTimes`) only
/// update at clock-tick granularity (commonly ~15ms). A bare passthrough
/// transform does negligible real work, so its true CPU usage can round down
/// to exactly zero over the whole test. Burning 1ms of real CPU per event,
/// across `EVENT_COUNT` events, guarantees the cumulative usage comfortably
/// exceeds that granularity on every supported platform.
const CPU_BURN_MS: u64 = 1;

fn has_transform_tags(metric: &Metric, transform_id: &str) -> bool {
    metric.tags().is_some_and(|tags| {
        tags.get("component_id") == Some(transform_id)
            && tags.get("component_type") == Some(TRANSFORM_TYPE)
            && tags.get("component_kind") == Some(TRANSFORM_KIND)
    })
}

fn assert_cpu_counter_positive(metrics: &[Metric], transform_id: &str) {
    let cpu_metric = metrics
        .iter()
        .find(|m| m.name() == "component_cpu_usage_ns_total" && has_transform_tags(m, transform_id))
        .unwrap_or_else(|| {
            panic!(
                "component_cpu_usage_ns_total not found for transform '{transform_id}'; \
                 available metrics: {:?}",
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

/// Builds and runs a source → transform → sink topology, sends `EVENT_COUNT`
/// events through it, and returns the captured metrics after the topology
/// stops.
///
/// `transform_id` is unique per call-site so that `has_transform_tags` can
/// discriminate this topology's metrics from those of concurrently-running
/// tests that share the same process-wide registry.
///
/// `measure_cpu_usage` controls the transform's `measure_cpu_usage` flag.
///
/// `make_config` receives a base `NoopTransformConfig` and can apply extra
/// options (e.g. `.with_concurrency()`) before the topology is assembled.
async fn run_cpu_topology(
    transform_id: &str,
    measure_cpu_usage: bool,
    make_config: impl FnOnce(NoopTransformConfig) -> NoopTransformConfig,
) -> Vec<Metric> {
    trace_init();

    // Derive unique source/sink IDs from the transform ID so every parallel
    // topology uses a fully disjoint set of component names.
    let source_id = format!("{transform_id}_source");
    let sink_id = format!("{transform_id}_sink");

    let controller = Controller::get().expect("metrics controller");

    let (mut source_tx, source_config) = basic_source();
    let (sink_done_tx, sink_done_rx) = oneshot::channel();
    let sink_config = CompletionSinkConfig::new(EVENT_COUNT, sink_done_tx);

    let mut config = Config::builder();
    config.add_source(&source_id, source_config);

    // Add a plain noop transform first, then flip the measure_cpu_usage flag on
    // the TransformOuter that the builder just inserted.
    config.add_transform(
        transform_id,
        &[source_id.as_str()],
        make_config(NoopTransformConfig::from(TransformType::Function)),
    );
    config
        .transforms
        .get_mut(&ComponentKey::from(transform_id))
        .expect("transform not found in builder")
        .measure_cpu_usage = measure_cpu_usage;

    config.add_sink(&sink_id, &[transform_id], sink_config);

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

/// Function transform (non-concurrent, inline runner): `component_cpu_usage_ns_total`
/// must be present with the correct component tags and a positive value when
/// `measure_cpu_usage = true`.
#[tokio::test]
async fn component_cpu_usage_emitted_function_transform() {
    let id = "cpu_transform_fn";
    let metrics = run_cpu_topology(id, true, |c| c.with_cpu_burn_ms(CPU_BURN_MS)).await;
    assert_cpu_counter_positive(&metrics, id);
}

/// Task transform (non-concurrent): the same `cpu_timed` wrapper is applied to
/// the task future in `build_task_transform`, so the counter must also be
/// emitted when `measure_cpu_usage = true`.
#[tokio::test]
async fn component_cpu_usage_emitted_task_transform() {
    let id = "cpu_transform_task";
    let metrics = run_cpu_topology(id, true, |_| {
        NoopTransformConfig::from(TransformType::Task).with_cpu_burn_ms(CPU_BURN_MS)
    })
    .await;
    assert_cpu_counter_positive(&metrics, id);
}

/// Concurrent sync transform: the driver future goes through `run_concurrently()`
/// in `build_sync_transform`, and each spawned batch task is wrapped via
/// `spawn_timed`. The counter must still be emitted when `measure_cpu_usage = true`.
#[tokio::test]
async fn component_cpu_usage_emitted_concurrent_transform() {
    let id = "cpu_transform_concurrent";
    let metrics = run_cpu_topology(id, true, |c| {
        c.with_concurrency().with_cpu_burn_ms(CPU_BURN_MS)
    })
    .await;
    assert_cpu_counter_positive(&metrics, id);
}

/// When `measure_cpu_usage = false` (the default), no
/// `component_cpu_usage_ns_total` counter should be emitted for the transform.
#[tokio::test]
async fn component_cpu_usage_not_emitted_without_measure_cpu_usage() {
    let id = "cpu_transform_no_measure";
    let metrics = run_cpu_topology(id, false, |c| c).await;

    let found = metrics
        .iter()
        .any(|m| m.name() == "component_cpu_usage_ns_total" && has_transform_tags(m, id));

    assert!(
        !found,
        "component_cpu_usage_ns_total should NOT be emitted when measure_cpu_usage = false"
    );
}
