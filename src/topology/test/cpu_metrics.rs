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
const SINK_TYPE: &str = "test_completion";
const SINK_KIND: &str = "sink";

/// Per-event busy-spin duration used by the CPU-usage tests below.
///
/// OS-level thread CPU-time clocks (e.g. Windows' `GetThreadTimes`) only
/// update at clock-tick granularity (commonly ~15ms). A bare passthrough
/// transform does negligible real work, so its true CPU usage can round down
/// to exactly zero over the whole test. Burning 1ms of real CPU per event,
/// across `EVENT_COUNT` events, guarantees the cumulative usage comfortably
/// exceeds that granularity on every supported platform.
const CPU_BURN_MS: u64 = 1;

/// CPU burned once by the sink healthcheck. Large enough to dwarf what the sink
/// itself spends on `EVENT_COUNT` events.
const HEALTHCHECK_CPU_BURN_MS: u32 = 100;

fn has_component_tags(metric: &Metric, kind: &str, component_type: &str, id: &str) -> bool {
    metric.tags().is_some_and(|tags| {
        tags.get("component_id") == Some(id)
            && tags.get("component_type") == Some(component_type)
            && tags.get("component_kind") == Some(kind)
    })
}

fn has_transform_tags(metric: &Metric, transform_id: &str) -> bool {
    has_component_tags(metric, TRANSFORM_KIND, TRANSFORM_TYPE, transform_id)
}

fn has_sink_tags(metric: &Metric, sink_id: &str) -> bool {
    has_component_tags(metric, SINK_KIND, SINK_TYPE, sink_id)
}

fn assert_cpu_counter_positive(metrics: &[Metric], transform_id: &str) {
    assert_component_cpu_counter_positive(metrics, transform_id, has_transform_tags);
}

fn assert_component_cpu_counter_positive(
    metrics: &[Metric],
    id: &str,
    has_tags: impl Fn(&Metric, &str) -> bool,
) {
    let value = component_cpu_counter(metrics, id, has_tags);
    assert!(
        value > 0.0,
        "expected component_cpu_usage_ns_total > 0, got {value}"
    );
}

fn component_cpu_counter(
    metrics: &[Metric],
    id: &str,
    has_tags: impl Fn(&Metric, &str) -> bool,
) -> f64 {
    let cpu_metric = metrics
        .iter()
        .find(|m| m.name() == "component_cpu_usage_ns_total" && has_tags(m, id))
        .unwrap_or_else(|| {
            panic!(
                "component_cpu_usage_ns_total not found for component '{id}'; \
                 available metrics: {:?}",
                metrics
                    .iter()
                    .map(Metric::name)
                    .collect::<std::collections::BTreeSet<_>>(),
            )
        });

    match cpu_metric.value() {
        MetricValue::Counter { value } => *value,
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
/// `measure_sink_cpu_usage` controls the sink's.
///
/// `make_config` and `make_sink` receive the base transform and sink configs and
/// can apply extra options (e.g. `.with_concurrency()`) before the topology is
/// assembled.
async fn run_cpu_topology(
    transform_id: &str,
    measure_cpu_usage: bool,
    measure_sink_cpu_usage: bool,
    make_config: impl FnOnce(NoopTransformConfig) -> NoopTransformConfig,
    make_sink: impl FnOnce(CompletionSinkConfig) -> CompletionSinkConfig,
) -> Vec<Metric> {
    trace_init();

    // Derive unique source/sink IDs from the transform ID so every parallel
    // topology uses a fully disjoint set of component names.
    let source_id = format!("{transform_id}_source");
    let sink_id = format!("{transform_id}_sink");

    let controller = Controller::get().expect("metrics controller");

    let (mut source_tx, source_config) = basic_source();
    let (sink_done_tx, sink_done_rx) = oneshot::channel();
    let sink_config = make_sink(CompletionSinkConfig::new(EVENT_COUNT, sink_done_tx));

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
    config
        .sinks
        .get_mut(&ComponentKey::from(sink_id.as_str()))
        .expect("sink not found in builder")
        .measure_cpu_usage = measure_sink_cpu_usage;

    let (topology, _) = start_topology(config.build().unwrap(), true).await;

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
    let metrics =
        run_cpu_topology(id, true, false, |c| c.with_cpu_burn_ms(CPU_BURN_MS), |s| s).await;
    assert_cpu_counter_positive(&metrics, id);
}

/// Task transform (non-concurrent): the same `cpu_timed` wrapper is applied to
/// the task future in `build_task_transform`, so the counter must also be
/// emitted when `measure_cpu_usage = true`.
#[tokio::test]
async fn component_cpu_usage_emitted_task_transform() {
    let id = "cpu_transform_task";
    let metrics = run_cpu_topology(
        id,
        true,
        false,
        |_| NoopTransformConfig::from(TransformType::Task).with_cpu_burn_ms(CPU_BURN_MS),
        |s| s,
    )
    .await;
    assert_cpu_counter_positive(&metrics, id);
}

/// Concurrent sync transform: the driver future goes through `run_concurrently()`
/// in `build_sync_transform`, and each spawned batch task is wrapped via
/// `spawn_timed`. The counter must still be emitted when `measure_cpu_usage = true`.
#[tokio::test]
async fn component_cpu_usage_emitted_concurrent_transform() {
    let id = "cpu_transform_concurrent";
    let metrics = run_cpu_topology(
        id,
        true,
        false,
        |c| c.with_concurrency().with_cpu_burn_ms(CPU_BURN_MS),
        |s| s,
    )
    .await;
    assert_cpu_counter_positive(&metrics, id);
}

/// When `measure_cpu_usage = false` (the default), no
/// `component_cpu_usage_ns_total` counter should be emitted for the transform.
#[tokio::test]
async fn component_cpu_usage_not_emitted_without_measure_cpu_usage() {
    let id = "cpu_transform_no_measure";
    let metrics = run_cpu_topology(id, false, false, |c| c, |s| s).await;

    let sink_id = format!("{id}_sink");
    let found = metrics.iter().any(|m| {
        m.name() == "component_cpu_usage_ns_total"
            && (has_transform_tags(m, id) || has_sink_tags(m, &sink_id))
    });

    assert!(
        !found,
        "component_cpu_usage_ns_total should NOT be emitted when measure_cpu_usage = false"
    );
}

/// Sink: the sink task is timed, and the CPU of the subtasks it spawns with
/// `spawn_timed` is charged to the same counter.
#[tokio::test]
async fn component_cpu_usage_emitted_sink() {
    let id = "cpu_sink";
    let metrics =
        run_cpu_topology(id, false, true, |c| c, |s| s.with_cpu_burn_ms(CPU_BURN_MS)).await;
    assert_component_cpu_counter_positive(&metrics, &format!("{id}_sink"), has_sink_tags);
}

/// Sink: connections opened by the healthcheck can be pooled and reused by the
/// sink, so the tasks it spawns are charged to the sink's counter too.
#[tokio::test]
async fn component_cpu_usage_includes_sink_healthcheck_tasks() {
    let id = "cpu_sink_healthcheck";
    let metrics = run_cpu_topology(
        id,
        false,
        true,
        |c| c,
        |s| s.with_healthcheck_cpu_burn_ms(u64::from(HEALTHCHECK_CPU_BURN_MS)),
    )
    .await;

    // Half the burn, to absorb preemption and Windows' clock-tick granularity.
    let min_ns = f64::from(HEALTHCHECK_CPU_BURN_MS) * 1e6 / 2.0;
    let value = component_cpu_counter(&metrics, &format!("{id}_sink"), has_sink_tags);
    assert!(
        value >= min_ns,
        "expected component_cpu_usage_ns_total >= {min_ns}, got {value}"
    );
}
