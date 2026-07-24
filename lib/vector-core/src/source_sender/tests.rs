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
        log.insert(event_path!("timestamp"), timestamp);
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
    let events: Vec<Event> = (0..(chunk_size_events() + expected_drop))
        .map(|_| {
            Event::Metric(Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 123.4 },
            ))
        })
        .collect();

    // `chunk_size_events()` events will be sent into buffer but then the future will not be polled to completion.
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

/// Build a sender with the given post-processor using the `set_post_processor` modifier.
fn make_sender_with_post_processor(
    pp: &Arc<dyn PostProcessor>,
) -> (SourceSender, impl futures::Stream<Item = Event> + Unpin) {
    let (mut sender, rx) = SourceSender::new_test_sender_with_options(TEST_BUFFER_SIZE, None);
    sender.set_post_processor(pp);
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

/// Processor that inserts a boolean field into every log event, counting calls.
struct InsertFieldProcessor {
    call_count: Arc<AtomicUsize>,
}

impl PostProcessor for InsertFieldProcessor {
    fn process_log(&self, event: &mut LogEvent) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        event.insert(event_path!("post_processed"), true);
    }

    fn process_metric(&self, _event: &mut Metric) {}

    fn process_trace(&self, _event: &mut TraceEvent) {}
}

#[tokio::test]
async fn post_processor_mutates_log_events() {
    // A processor should mutate every log event that flows through.
    metrics::init_test();

    let call_count = Arc::new(AtomicUsize::new(0));
    let pp = InsertFieldProcessor {
        call_count: Arc::clone(&call_count),
    };

    let pp: Arc<dyn PostProcessor> = Arc::new(pp);
    let (mut sender, mut stream) = make_sender_with_post_processor(&pp);

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

/// Processor that counts calls per event type to verify typed dispatch.
struct DispatchCountProcessor {
    logs: Arc<AtomicUsize>,
    metrics: Arc<AtomicUsize>,
    traces: Arc<AtomicUsize>,
}

impl PostProcessor for DispatchCountProcessor {
    fn process_log(&self, _event: &mut LogEvent) {
        self.logs.fetch_add(1, Ordering::SeqCst);
    }

    fn process_metric(&self, _event: &mut Metric) {
        self.metrics.fetch_add(1, Ordering::SeqCst);
    }

    fn process_trace(&self, _event: &mut TraceEvent) {
        self.traces.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn post_processor_dispatches_by_event_type() {
    // Verify that each event type is routed to the correct trait method.
    metrics::init_test();

    let logs = Arc::new(AtomicUsize::new(0));
    let metrics = Arc::new(AtomicUsize::new(0));
    let traces = Arc::new(AtomicUsize::new(0));

    let pp = DispatchCountProcessor {
        logs: Arc::clone(&logs),
        metrics: Arc::clone(&metrics),
        traces: Arc::clone(&traces),
    };

    let pp: Arc<dyn PostProcessor> = Arc::new(pp);
    let (mut sender, _stream) = make_sender_with_post_processor(&pp);

    sender
        .send_event(Event::Log(LogEvent::default()))
        .await
        .expect("log send should succeed");
    sender
        .send_event(Event::Metric(Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        )))
        .await
        .expect("metric send should succeed");
    sender
        .send_event(Event::Trace(TraceEvent::default()))
        .await
        .expect("trace send should succeed");
    drop(sender);

    assert_eq!(logs.load(Ordering::SeqCst), 1, "process_log called once");
    assert_eq!(
        metrics.load(Ordering::SeqCst),
        1,
        "process_metric called once"
    );
    assert_eq!(
        traces.load(Ordering::SeqCst),
        1,
        "process_trace called once"
    );
}

/// Processor that replaces the entire inner log event with a default, simulating the worst-case
/// whole-event replacement that would previously drop all EventMetadata fields.
struct ReplaceWithDefaultProcessor;

impl PostProcessor for ReplaceWithDefaultProcessor {
    fn process_log(&self, event: &mut LogEvent) {
        *event = LogEvent::default();
    }

    fn process_metric(&self, _event: &mut Metric) {}

    fn process_trace(&self, _event: &mut TraceEvent) {}
}

#[tokio::test]
async fn post_processor_whole_event_replacement_preserves_finalizers() {
    // A processor that replaces the entire inner event (e.g. `*log = LogEvent::default()`) must
    // not drop the event's finalizers (batch-ack notifiers). Metadata fields outside the inner
    // value (e.g. `datadog_api_key`) are NOT preserved — the processor takes full ownership of
    // the event when it replaces the inner value.
    metrics::init_test();

    let pp: Arc<dyn PostProcessor> = Arc::new(ReplaceWithDefaultProcessor);
    let (mut sender, mut stream) = make_sender_with_post_processor(&pp);

    let mut log = LogEvent::default();
    log.metadata_mut()
        .set_datadog_api_key(Arc::from("test-api-key"));
    log.insert(
        event_path!("original_field"),
        "should_be_gone_after_replace",
    );

    sender
        .send_event(Event::Log(log))
        .await
        .expect("send should succeed");
    drop(sender);

    let event = stream.next().await.expect("expected one event");
    let log = event.as_log();

    // The processor replaced the inner event, so the original field is gone — expected.
    assert!(
        log.get(event_path!("original_field")).is_none(),
        "field added before replacement should not be present"
    );

    // Metadata set before the replacement is also gone — the processor owns the new event.
    assert_eq!(
        log.metadata().datadog_api_key().as_deref(),
        None,
        "datadog_api_key is not preserved when the processor replaces the entire inner value"
    );
}
