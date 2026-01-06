use tokio::{
    sync::oneshot,
    time::{Duration, timeout},
};
use vector_lib::metrics::Controller;

use crate::{
    config::Config,
    event::{Event, LogEvent, Metric, MetricValue},
    test_util::{
        mock::{basic_source, completion_sink, noop_transform},
        start_topology, trace_init,
    },
};

#[tokio::test]
async fn sink_processing_time_metrics_emitted() {
    trace_init();

    let controller = Controller::get().expect("metrics controller");
    controller.reset();

    let event_count = 3;

    let (mut source_tx, source_config) = basic_source();
    let transform_config = noop_transform();
    let (sink_done_tx, sink_done_rx) = oneshot::channel();
    let sink_config = completion_sink(event_count, sink_done_tx);

    let mut config = Config::builder();
    config.add_source("latency_source", source_config);
    config.add_transform("latency_delay", &["latency_source"], transform_config);
    config.add_sink("latency_sink", &["latency_delay"], sink_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    for idx in 0..event_count {
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

    let metrics = controller.capture_metrics();
    let sink_id = "latency_sink";
    let source_id = "latency_source";

    let histogram = metrics
        .iter()
        .find(|metric| {
            metric.name() == "event_processing_time_seconds"
                && has_latency_tags(metric, sink_id, source_id)
        })
        .expect("event_processing_time_seconds histogram missing");

    match histogram.value() {
        MetricValue::AggregatedHistogram { count, .. } => {
            assert_eq!(
                *count, event_count as u64,
                "histogram count should match number of events"
            );
        }
        other => panic!("expected aggregated histogram, got {other:?}"),
    }

    let gauge = metrics
        .iter()
        .find(|metric| {
            metric.name() == "event_processing_time_mean_seconds"
                && has_latency_tags(metric, sink_id, source_id)
        })
        .expect("event_processing_time_mean_seconds gauge missing");

    match gauge.value() {
        MetricValue::Gauge { value } => {
            assert!(
                *value >= 0.0,
                "expected mean latency to be non-negative, got {value}"
            );
        }
        other => panic!("expected gauge metric, got {other:?}"),
    }
}

fn has_latency_tags(metric: &Metric, sink: &str, source: &str) -> bool {
    metric.tags().is_some_and(|tags| {
        tags.get("source_component_id") == Some(source)
            && tags.get("sink_component_id") == Some(sink)
    })
}
