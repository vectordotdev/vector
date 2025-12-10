use chrono::{DateTime, Duration, Utc};
use rand::{Rng, rng};
use std::time::{Duration as StdDuration, Instant};
use tokio::time::timeout;
use vrl::event_path;

use super::*;
use crate::{
    event::{Event, LogEvent, Metric, MetricKind, MetricValue, TraceEvent},
    metrics::{self, Controller},
};

#[tokio::test]
async fn emits_lag_time_for_log() {
    emit_and_test(|timestamp| {
        let mut log = LogEvent::from("Log message");
        log.insert("timestamp", timestamp);
        Event::Log(log)
    })
    .await;
}

#[tokio::test]
async fn emits_lag_time_for_metric() {
    emit_and_test(|timestamp| {
        Event::Metric(
            Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 123.4 },
            )
            .with_timestamp(Some(timestamp)),
        )
    })
    .await;
}

#[tokio::test]
async fn emits_lag_time_for_trace() {
    emit_and_test(|timestamp| {
        let mut trace = TraceEvent::default();
        trace.insert(event_path!("timestamp"), timestamp);
        Event::Trace(trace)
    })
    .await;
}

async fn emit_and_test(make_event: impl FnOnce(DateTime<Utc>) -> Event) {
    metrics::init_test();
    let (mut sender, _stream) = SourceSender::new_test();
    let millis = rng().random_range(10..10000);
    let timestamp = Utc::now() - Duration::milliseconds(millis);
    #[expect(clippy::cast_precision_loss)]
    let expected = millis as f64 / 1000.0;

    let event = make_event(timestamp);
    sender
        .send_event(event)
        .await
        .expect("Send should not fail");

    let lag_times = Controller::get()
        .expect("There must be a controller")
        .capture_metrics()
        .into_iter()
        .filter(|metric| metric.name() == "source_lag_time_seconds")
        .collect::<Vec<_>>();
    assert_eq!(lag_times.len(), 1);

    let lag_time = &lag_times[0];
    match lag_time.value() {
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            let mut done = false;
            for bucket in buckets {
                if !done && bucket.upper_limit >= expected {
                    assert_eq!(bucket.count, 1);
                    done = true;
                } else {
                    assert_eq!(bucket.count, 0);
                }
            }
            assert_eq!(*count, 1);
            assert!(
                (*sum - expected).abs() <= 0.002,
                "Histogram sum does not match expected sum: {} vs {}",
                *sum,
                expected,
            );
        }
        _ => panic!("source_lag_time_seconds has invalid type"),
    }
}

#[tokio::test]
async fn emits_component_discarded_events_total_for_send_event() {
    metrics::init_test();
    let (mut sender, _recv) = SourceSender::new_test_sender_with_options(1, None);

    let event = Event::Metric(Metric::new(
        "name",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 123.4 },
    ));

    // First send will succeed.
    sender
        .send_event(event.clone())
        .await
        .expect("First send should not fail");

    // Second send will timeout, so the future will not be polled to completion.
    let res = timeout(
        std::time::Duration::from_millis(100),
        sender.send_event(event.clone()),
    )
    .await;
    assert!(res.is_err(), "Send should have timed out.");

    let component_discarded_events_total = Controller::get()
        .expect("There must be a controller")
        .capture_metrics()
        .into_iter()
        .filter(|metric| metric.name() == "component_discarded_events_total")
        .collect::<Vec<_>>();
    assert_eq!(component_discarded_events_total.len(), 1);

    let component_discarded_events_total = &component_discarded_events_total[0];
    let MetricValue::Counter { value } = component_discarded_events_total.value() else {
        panic!("component_discarded_events_total has invalid type")
    };
    assert_eq!(*value, 1.0);
}

#[tokio::test]
#[expect(clippy::cast_precision_loss)]
async fn emits_component_discarded_events_total_for_send_batch() {
    metrics::init_test();
    let (mut sender, _recv) = SourceSender::new_test_sender_with_options(1, None);

    let expected_drop = 100;
    let events: Vec<Event> = (0..(CHUNK_SIZE + expected_drop))
        .map(|_| {
            Event::Metric(Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 123.4 },
            ))
        })
        .collect();

    // `CHUNK_SIZE` events will be sent into buffer but then the future will not be polled to completion.
    let res = timeout(
        std::time::Duration::from_millis(100),
        sender.send_batch(events),
    )
    .await;
    assert!(res.is_err(), "Send should have timed out.");

    let metrics = get_component_metrics();
    assert_no_metric(&metrics, "component_timed_out_events_total");
    assert_no_metric(&metrics, "component_timed_out_requests_total");
    assert_counter_metric(
        &metrics,
        "component_discarded_events_total",
        expected_drop as f64,
    );
}

#[tokio::test]
async fn times_out_send_event_with_timeout() {
    metrics::init_test();

    let timeout_duration = StdDuration::from_millis(10);
    let (mut sender, _recv) = SourceSender::new_test_sender_with_options(1, Some(timeout_duration));

    let event = Event::Metric(Metric::new(
        "name",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 123.4 },
    ));

    sender
        .send_event(event.clone())
        .await
        .expect("First send should succeed");

    let start = Instant::now();
    let result = sender.send_event(event).await;
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(SendError::Timeout)),
        "Send should return a timeout error."
    );
    assert!(
        elapsed >= timeout_duration,
        "Send did not wait for the configured timeout"
    );
    assert!(elapsed <= timeout_duration * 2, "Send waited too long");

    let metrics = get_component_metrics();
    assert_no_metric(&metrics, "component_discarded_events_total");
    assert_counter_metric(&metrics, "component_timed_out_events_total", 1.0);
    assert_counter_metric(&metrics, "component_timed_out_requests_total", 1.0);
}

fn get_component_metrics() -> Vec<Metric> {
    Controller::get()
        .expect("There must be a controller")
        .capture_metrics()
        .into_iter()
        .filter(|metric| metric.name().starts_with("component_"))
        .collect()
}

fn assert_no_metric(metrics: &[Metric], name: &str) {
    assert!(
        !metrics.iter().any(|metric| metric.name() == name),
        "Metric {name} should not be present"
    );
}

fn assert_counter_metric(metrics: &[Metric], name: &str, expected: f64) {
    let mut filter = metrics.iter().filter(|metric| metric.name() == name);
    let Some(metric) = filter.next() else {
        panic!("Metric {name} should be present");
    };
    let MetricValue::Counter { value } = metric.value() else {
        panic!("Metric {name} should be a counter");
    };
    assert_eq!(*value, expected);
    assert!(
        filter.next().is_none(),
        "Only one {name} metric should be present"
    );
}

#[tokio::test]
#[expect(clippy::cast_precision_loss)]
async fn emits_buffer_utilization_histogram_on_send_and_receive() {
    metrics::init_test();
    let buffer_size = 2;
    let (mut sender, mut recv) = SourceSender::new_test_sender_with_options(buffer_size, None);

    let event = Event::Log(LogEvent::from("test event"));
    sender
        .send_event(event.clone())
        .await
        .expect("first send succeeds");
    sender
        .send_event(event)
        .await
        .expect("second send succeeds");

    // Drain the channel so both the send and receive paths are exercised.
    assert!(recv.next().await.is_some());
    assert!(recv.next().await.is_some());

    let metrics: Vec<_> = Controller::get()
        .expect("metrics controller available")
        .capture_metrics()
        .into_iter()
        .filter(|metric| metric.name().starts_with("source_buffer_"))
        .collect();
    assert_eq!(metrics.len(), 3, "expected 3 utilization metrics");

    let find_metric = |name: &str| {
        metrics
            .iter()
            .find(|m| m.name() == name)
            .unwrap_or_else(|| panic!("missing metric: {name}"))
    };

    let metric = find_metric("source_buffer_utilization");
    let tags = metric.tags().expect("utilization histogram has tags");
    assert_eq!(tags.get("output"), Some("_default"));

    let metric = find_metric("source_buffer_utilization_level");
    let MetricValue::Gauge { value } = metric.value() else {
        panic!("source_buffer_utilization_level should be a gauge");
    };
    assert_eq!(*value, 2.0);

    let metric = find_metric("source_buffer_max_event_size");
    let MetricValue::Gauge { value } = metric.value() else {
        panic!("source_buffer_max_event_size should be a gauge");
    };
    assert_eq!(*value, buffer_size as f64);
}
