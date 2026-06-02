use chrono::{DateTime, Duration, Utc};
use futures::StreamExt as _;
use rand::{Rng, rng};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration as StdDuration, Instant};
use tokio::time::timeout;
use vrl::event_path;

use super::*;
use crate::{
    event::{Event, LogEvent, Metric, MetricKind, MetricValue, TraceEvent, into_event_stream},
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
async fn emits_buffer_utilization_histogram_on_send_and_receive() {
    const BUFFER_SIZE: usize = 2;

    metrics::init_test();
    let (mut sender, mut recv) = SourceSender::new_test_sender_with_options(BUFFER_SIZE, None);

    let event = Event::Log(LogEvent::from("test event"));
    sender
        .send_event(event.clone())
        .await
        .expect("first send succeeds");
    sender
        .send_event(event)
        .await
        .expect("second send succeeds");

    assert_buffer_metrics(BUFFER_SIZE, 2);

    // Drain the channel so both the send and receive paths are exercised.
    assert!(recv.next().await.is_some());
    assert!(recv.next().await.is_some());

    assert_buffer_metrics(BUFFER_SIZE, 0);
}

#[expect(clippy::cast_precision_loss)]
fn assert_buffer_metrics(buffer_size: usize, level: usize) {
    let metrics: Vec<_> = Controller::get()
        .expect("metrics controller available")
        .capture_metrics()
        .into_iter()
        .filter(|metric| metric.name().starts_with("source_buffer_"))
        .collect();
    assert_eq!(metrics.len(), 5, "expected 5 utilization metrics");

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
    assert_eq!(*value, level as f64);

    let metric = find_metric("source_buffer_max_event_size");
    let MetricValue::Gauge { value } = metric.value() else {
        panic!("source_buffer_max_event_size should be a gauge");
    };
    assert_eq!(*value, buffer_size as f64);

    let metric = find_metric("source_buffer_max_size_events");
    let MetricValue::Gauge { value } = metric.value() else {
        panic!("source_buffer_max_size_events should be a gauge");
    };
    assert_eq!(*value, buffer_size as f64);
}

// ── PostProcessor tests ──────────────────────────────────────────────────────

/// Helper: build a SourceSender with the given PostProcessor and return (sender, event stream).
fn make_sender_with_post_processor(
    pp: PostProcessor,
) -> (SourceSender, impl futures::Stream<Item = Event> + Unpin) {
    let (sender, rx) = SourceSender::new_test_with_post_processor(pp);
    let stream = rx.into_stream().flat_map(into_event_stream);
    (sender, stream)
}

#[tokio::test]
async fn post_processor_none_is_noop() {
    // With no post-processor events should pass through unchanged.
    let (mut sender, mut stream) = SourceSender::new_test();

    let mut log = LogEvent::default();
    log.insert(event_path!("hello"), "world");
    sender
        .send_event(Event::Log(log))
        .await
        .expect("send should succeed");
    drop(sender);

    let event = stream.next().await.expect("expected one event");
    let log = event.as_log();
    assert_eq!(
        log.get(event_path!("hello")),
        Some(&vrl::value::Value::from("world"))
    );
    assert!(log.get(event_path!("post_processed")).is_none());
}

#[tokio::test]
async fn post_processor_hard_coded_mutates_events() {
    // A HardCoded processor should mutate every event that flows through.
    metrics::init_test();

    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let pp = PostProcessor::HardCoded(Arc::new(move |event: &mut Event| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        if let Event::Log(log) = event {
            log.insert(event_path!("post_processed"), true);
        }
    }));

    let (mut sender, mut stream) = make_sender_with_post_processor(pp);

    let mut log = LogEvent::default();
    log.insert(event_path!("original"), "yes");
    sender
        .send_event(Event::Log(log))
        .await
        .expect("send should succeed");
    drop(sender);

    let event = stream.next().await.expect("expected one event");
    let log = event.as_log();

    assert_eq!(
        log.get(event_path!("original")),
        Some(&vrl::value::Value::from("yes")),
        "original field must be preserved"
    );
    assert_eq!(
        log.get(event_path!("post_processed")),
        Some(&vrl::value::Value::Boolean(true)),
        "post_processed field must be set by the processor"
    );
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "processor must be called exactly once"
    );
}

/// Verify that a closure which preserves the variant (mutates fields but does not change log →
/// metric etc.) passes the debug_assert contract without panicking.
#[tokio::test]
async fn post_processor_variant_preserved_does_not_panic() {
    // This test documents the contract: same-variant mutation is always safe.
    let pp = PostProcessor::HardCoded(Arc::new(|event: &mut Event| {
        // Mutate fields but keep the same variant.
        if let Event::Log(log) = event {
            log.insert(event_path!("contract"), "ok");
        }
    }));

    let (mut sender, mut stream) = make_sender_with_post_processor(pp);

    let mut log = LogEvent::default();
    log.insert(event_path!("x"), 1_i64);
    sender
        .send_event(Event::Log(log))
        .await
        .expect("send should succeed");
    drop(sender);

    let event = stream.next().await.expect("expected one event");
    assert_eq!(
        event.as_log().get(event_path!("contract")),
        Some(&vrl::value::Value::from("ok")),
        "mutation inside same variant must be visible downstream"
    );
}

/// In debug builds, a closure that changes the event variant must panic.
#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic(expected = "PostProcessor::HardCoded closure changed the event variant")]
async fn post_processor_variant_change_panics_in_debug() {
    let pp = PostProcessor::HardCoded(Arc::new(|event: &mut Event| {
        // Intentionally violate the contract: replace a Log with a Metric.
        *event = Event::Metric(Metric::new(
            "bad",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 0.0 },
        ));
    }));

    let (mut sender, _stream) = make_sender_with_post_processor(pp);

    let log = LogEvent::default();
    // This should panic inside send_event due to the debug_assert.
    sender.send_event(Event::Log(log)).await.ok();
}
