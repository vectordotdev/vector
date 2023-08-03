use futures::SinkExt;
use futures::StreamExt;
use tokio::sync::watch;

use super::*;
use crate::api::schema::events::notification::{EventNotification, EventNotificationType};
use crate::api::schema::events::output::OutputEventsPayload;
use crate::api::schema::events::{create_events_stream, log, metric};
use crate::config::Config;
use crate::event::{Metric, MetricKind, MetricValue};
use crate::sinks::blackhole::BlackholeConfig;
use crate::sources::demo_logs::{DemoLogsConfig, OutputFormat};
use crate::test_util::start_topology;
use crate::transforms::log_to_metric::{GaugeConfig, LogToMetricConfig, MetricConfig};
use crate::transforms::remap::RemapConfig;

#[test]
/// Patterns should accept globbing.
fn matches() {
    let patterns = ["ab*", "12?", "xy?"];

    // Should find.
    for id in &["abc", "123", "xyz"] {
        assert!(patterns.iter().any(|p| p.to_string().matches_glob(id)));
    }

    // Should not find.
    for id in &["xzy", "ad*", "1234"] {
        assert!(!patterns.iter().any(|p| p.to_string().matches_glob(id)));
    }
}

#[tokio::test]
/// A tap sink should match a pattern, receive the correct notifications,
/// and receive events
async fn sink_events() {
    let pattern_matched = "tes*";
    let pattern_not_matched = "xyz";
    let id = OutputId::from(&ComponentKey::from("test"));

    let (mut fanout, control_tx) = fanout::Fanout::new();
    let mut outputs = HashMap::new();
    outputs.insert(id.clone(), control_tx);

    let (watch_tx, watch_rx) = watch::channel(HashMap::new());
    let (sink_tx, mut sink_rx) = tokio_mpsc::channel(10);

    let _controller = TapController::new(
        watch_rx,
        sink_tx,
        &[pattern_matched.to_string(), pattern_not_matched.to_string()],
    );

    // Add the outputs to trigger a change event.
    watch_tx.send(outputs).unwrap();

    // First two events should contain a notification that one pattern matched, and
    // one that didn't.
    #[allow(clippy::eval_order_dependence)]
    let notifications = vec![sink_rx.recv().await, sink_rx.recv().await];

    for notification in notifications.into_iter() {
        match notification {
            Some(TapPayload::Notification(returned_id, TapNotification::Matched))
                if returned_id == pattern_matched =>
            {
                continue
            }
            Some(TapPayload::Notification(returned_id, TapNotification::NotMatched))
                if returned_id == pattern_not_matched =>
            {
                continue
            }
            _ => panic!("unexpected payload"),
        }
    }

    // Send some events down the wire. Waiting until the first notifications are in
    // to ensure the event handler has been initialized.
    let log_event = Event::from(LogEvent::default());
    let metric_event = Event::from(Metric::new(
        id.to_string(),
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    ));

    _ = fanout.send(metric_event).await.unwrap();
    _ = fanout.send(log_event).await.unwrap();

    // 3rd payload should be the metric event
    assert!(matches!(
            sink_rx.recv().await,
            Some(TapPayload::Metric(returned_id, _)) if returned_id == id
    ));

    // 4th payload should be the log event
    assert!(matches!(
            sink_rx.recv().await,
            Some(TapPayload::Log(returned_id, _)) if returned_id == id
    ));
}

fn assert_notification(payload: OutputEventsPayload) -> EventNotification {
    if let OutputEventsPayload::Notification(notification) = payload {
        notification
    } else {
        panic!("Expected payload to be a Notification")
    }
}

fn assert_log(payload: OutputEventsPayload) -> log::Log {
    if let OutputEventsPayload::Log(log) = payload {
        log
    } else {
        panic!("Expected payload to be a Log")
    }
}

fn assert_metric(payload: OutputEventsPayload) -> metric::Metric {
    if let OutputEventsPayload::Metric(metric) = payload {
        metric
    } else {
        panic!("Expected payload to be a Metric")
    }
}

#[tokio::test]
async fn integration_test_source_log() {
    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: 0.01,
            count: 200,
            format: OutputFormat::Json,
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["in"],
        BlackholeConfig {
            print_interval_secs: 1,
            rate: None,
        },
    );

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let source_tap_stream =
        create_events_stream(topology.watch(), vec!["in".to_string()], 500, 100);

    let source_tap_events: Vec<_> = source_tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(source_tap_events[0][0].clone()),
        EventNotification::new("in".to_string(), EventNotificationType::Matched)
    );
    let _log = assert_log(source_tap_events[1][0].clone());
}

#[tokio::test]
async fn integration_test_source_metric() {
    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: 0.01,
            count: 200,
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: vec!["1".to_string()],
            },
            ..Default::default()
        },
    );
    config.add_transform(
        "to_metric",
        &["in"],
        LogToMetricConfig {
            metrics: vec![MetricConfig::Gauge(GaugeConfig {
                field: "message".to_string(),
                name: None,
                namespace: None,
                tags: None,
            })],
        },
    );
    config.add_sink(
        "out",
        &["to_metric"],
        BlackholeConfig {
            print_interval_secs: 1,
            rate: None,
        },
    );

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let source_tap_stream =
        create_events_stream(topology.watch(), vec!["to_metric".to_string()], 500, 100);

    let source_tap_events: Vec<_> = source_tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(source_tap_events[0][0].clone()),
        EventNotification::new("to_metric".to_string(), EventNotificationType::Matched)
    );
    assert_metric(source_tap_events[1][0].clone());
}

#[tokio::test]
async fn integration_test_transform() {
    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: 0.01,
            count: 200,
            format: OutputFormat::Json,
            ..Default::default()
        },
    );
    config.add_transform(
        "transform",
        &["in"],
        RemapConfig {
            source: Some("".to_string()),
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["transform"],
        BlackholeConfig {
            print_interval_secs: 1,
            rate: None,
        },
    );

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let transform_tap_stream =
        create_events_stream(topology.watch(), vec!["transform".to_string()], 500, 100);

    let transform_tap_events: Vec<_> = transform_tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(transform_tap_events[0][0].clone()),
        EventNotification::new("transform".to_string(), EventNotificationType::Matched)
    );
    let _log = assert_log(transform_tap_events[1][0].clone());
}

#[tokio::test]
async fn integration_test_tap_non_default_output() {
    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: 0.01,
            count: 200,
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: vec!["test2".to_string()],
            },
            ..Default::default()
        },
    );
    config.add_transform(
        "transform",
        &["in"],
        RemapConfig {
            source: Some("assert_eq!(.message, \"test1\")".to_string()),
            drop_on_error: true,
            reroute_dropped: true,
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["transform"],
        BlackholeConfig {
            print_interval_secs: 1,
            rate: None,
        },
    );

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let transform_tap_remap_dropped_stream = create_events_stream(
        topology.watch(),
        vec!["transform.dropped".to_string()],
        500,
        100,
    );

    let transform_tap_events: Vec<_> = transform_tap_remap_dropped_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(transform_tap_events[0][0].clone()),
        EventNotification::new(
            "transform.dropped".to_string(),
            EventNotificationType::Matched
        )
    );
    assert_eq!(
        assert_log(transform_tap_events[1][0].clone())
            .get_message()
            .unwrap_or_default(),
        "test2"
    );
}

#[tokio::test]
async fn integration_test_tap_multiple_outputs() {
    let mut config = Config::builder();
    config.add_source(
        "in-test1",
        DemoLogsConfig {
            interval: 0.01,
            count: 1,
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: vec!["test1".to_string()],
            },
            ..Default::default()
        },
    );
    config.add_source(
        "in-test2",
        DemoLogsConfig {
            interval: 0.01,
            count: 1,
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: vec!["test2".to_string()],
            },
            ..Default::default()
        },
    );
    config.add_transform(
        "transform",
        &["in*"],
        RemapConfig {
            source: Some("assert_eq!(.message, \"test1\")".to_string()),
            drop_on_error: true,
            reroute_dropped: true,
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["transform"],
        BlackholeConfig {
            print_interval_secs: 1,
            rate: None,
        },
    );

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let mut transform_tap_all_outputs_stream =
        create_events_stream(topology.watch(), vec!["transform*".to_string()], 500, 100);

    let transform_tap_notifications = transform_tap_all_outputs_stream.next().await.unwrap();
    assert_eq!(
        assert_notification(transform_tap_notifications[0].clone()),
        EventNotification::new("transform*".to_string(), EventNotificationType::Matched)
    );

    let mut default_output_found = false;
    let mut dropped_output_found = false;
    for _ in 0..2 {
        if default_output_found && dropped_output_found {
            break;
        }

        match transform_tap_all_outputs_stream.next().await {
            Some(tap_events) => {
                if !default_output_found {
                    default_output_found = tap_events
                        .iter()
                        .map(|payload| assert_log(payload.clone()))
                        .any(|log| log.get_message().unwrap_or_default() == "test1");
                }
                if !dropped_output_found {
                    dropped_output_found = tap_events
                        .iter()
                        .map(|payload| assert_log(payload.clone()))
                        .any(|log| log.get_message().unwrap_or_default() == "test2");
                }
            }
            None => break,
        }
    }

    assert!(default_output_found && dropped_output_found);
}
