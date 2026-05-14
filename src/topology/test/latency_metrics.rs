use std::time::Instant;
use tokio::{
    sync::oneshot,
    time::{Duration, timeout},
};
use vector_lib::metrics::Controller;

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
const TRANSFORM_DELAY_MS: u64 = 10;
const SOURCE_ID: &str = "latency_source";
const TRANSFORM_ID: &str = "latency_delay";
const TRANSFORM_TYPE: &str = "test_noop";
const TRANSFORM_KIND: &str = "transform";
const SINK_ID: &str = "latency_sink";

struct LatencyTestRun {
    metrics: Vec<Metric>,
    elapsed_time: f64,
}

#[tokio::test]
async fn component_latency_metrics_emitted() {
    let run = run_latency_topology().await;

    assert_histogram_count(
        &run.metrics,
        "component_latency_seconds",
        has_component_tags,
    );
    assert_gauge_range(
        &run.metrics,
        "component_latency_mean_seconds",
        has_component_tags,
        TRANSFORM_DELAY_MS as f64 / 1000.0,
        run.elapsed_time,
    );
}

async fn run_latency_topology() -> LatencyTestRun {
    trace_init();

    let controller = Controller::get().expect("metrics controller");
    controller.reset();

    let (mut source_tx, source_config) = basic_source();
    let transform_config =
        NoopTransformConfig::from(TransformType::Task).with_delay_ms(TRANSFORM_DELAY_MS);
    let (sink_done_tx, sink_done_rx) = oneshot::channel();
    let sink_config = CompletionSinkConfig::new(EVENT_COUNT, sink_done_tx);

    let mut config = Config::builder();
    config.add_source(SOURCE_ID, source_config);
    config.add_transform(TRANSFORM_ID, &[SOURCE_ID], transform_config);
    config.add_sink(SINK_ID, &[TRANSFORM_ID], sink_config);

    let start_time = Instant::now();
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
    let elapsed_time = start_time.elapsed().as_secs_f64();

    LatencyTestRun {
        metrics: controller.capture_metrics(),
        elapsed_time,
    }
}

fn assert_histogram_count(metrics: &[Metric], metric_name: &str, tags_match: fn(&Metric) -> bool) {
    let histogram = metrics
        .iter()
        .find(|metric| metric.name() == metric_name && tags_match(metric))
        .unwrap_or_else(|| panic!("{metric_name} histogram missing"));

    match histogram.value() {
        MetricValue::AggregatedHistogram { count, .. } => {
            assert_eq!(
                *count, EVENT_COUNT as u64,
                "histogram count should match number of events"
            );
        }
        other => panic!("expected aggregated histogram, got {other:?}"),
    }
}

fn assert_gauge_range(
    metrics: &[Metric],
    metric_name: &str,
    tags_match: fn(&Metric) -> bool,
    expected_min: f64,
    elapsed_time: f64,
) {
    let gauge = metrics
        .iter()
        .find(|metric| metric.name() == metric_name && tags_match(metric))
        .unwrap_or_else(|| panic!("{metric_name} gauge missing"));

    match gauge.value() {
        MetricValue::Gauge { value } => {
            assert!(
                *value >= expected_min,
                "expected mean latency to be >= {expected_min}, got {value}"
            );
            assert!(
                *value < elapsed_time,
                "expected mean latency ({value}) to be less than elapsed time ({elapsed_time})"
            );
        }
        other => panic!("expected gauge metric, got {other:?}"),
    }
}

fn has_component_tags(metric: &Metric) -> bool {
    metric.tags().is_some_and(|tags| {
        tags.get("component_id") == Some(TRANSFORM_ID)
            && tags.get("component_type") == Some(TRANSFORM_TYPE)
            && tags.get("component_kind") == Some(TRANSFORM_KIND)
    })
}
