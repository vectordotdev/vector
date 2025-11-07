use chrono::{DateTime, Duration, Utc};
use rand::{Rng, rng};
use tokio::time::timeout;
use vector_lib::event::{Event, LogEvent, Metric, MetricKind, MetricValue, TraceEvent};
use vector_lib::metrics::{self, Controller};
use vrl::event_path;

use super::*;

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
    let (mut sender, _recv) = SourceSender::new_test_sender_with_buffer(1);

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
async fn emits_component_discarded_events_total_for_send_batch() {
    metrics::init_test();
    let (mut sender, _recv) = SourceSender::new_test_sender_with_buffer(1);

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
    assert_eq!(*value, expected_drop as f64);
}
