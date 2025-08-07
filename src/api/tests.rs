use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::api::schema::events::output::OutputEventsPayload;
use crate::api::schema::events::{create_events_stream, log, metric};
use crate::config::{Config, OutputId};
use crate::event::{LogEvent, Metric, MetricKind, MetricValue};
use crate::sinks::blackhole::BlackholeConfig;
use crate::sources::demo_logs::{DemoLogsConfig, OutputFormat};
use crate::test_util::{start_topology, trace_init};
use crate::transforms::log_to_metric::{LogToMetricConfig, MetricConfig, MetricTypeConfig};
use crate::transforms::remap::RemapConfig;
use futures::StreamExt;
use tokio::sync::{mpsc, watch};
use vector_lib::config::ComponentKey;
use vector_lib::fanout;
use vector_lib::tap::controller::{TapController, TapPatterns, TapPayload};
use vector_lib::tap::notification::{InvalidMatch, Matched, NotMatched, Notification};
use vector_lib::tap::topology::{TapOutput, TapResource};

#[tokio::test]
/// A tap sink should match a pattern, receive the correct notifications,
/// and receive events
async fn sink_events() {
    let pattern_matched = "tes*";
    let pattern_not_matched = "xyz";
    let id = OutputId::from(&ComponentKey::from("test"));

    let (mut fanout, control_tx) = fanout::Fanout::new();
    let mut outputs = HashMap::new();
    outputs.insert(
        TapOutput {
            output_id: id.clone(),
            component_kind: "source",
            component_type: "demo".to_string(),
        },
        control_tx,
    );
    let tap_resource = TapResource {
        outputs,
        inputs: HashMap::new(),
        source_keys: Vec::new(),
        sink_keys: Vec::new(),
        removals: HashSet::new(),
    };

    let (watch_tx, watch_rx) = watch::channel(TapResource::default());
    let (sink_tx, mut sink_rx) = mpsc::channel(10);

    let _controller = TapController::new(
        watch_rx,
        sink_tx,
        TapPatterns::new(
            HashSet::from([pattern_matched.to_string(), pattern_not_matched.to_string()]),
            HashSet::new(),
        ),
    );

    // Add the outputs to trigger a change event.
    watch_tx.send(tap_resource).unwrap();

    // First two events should contain a notification that one pattern matched, and
    // one that didn't.
    #[allow(clippy::mixed_read_write_in_expression)]
    let notifications = vec![sink_rx.recv().await, sink_rx.recv().await];

    for notification in notifications.into_iter() {
        match notification {
            Some(TapPayload::Notification(Notification::Matched(matched)))
                if matched.pattern == pattern_matched =>
            {
                continue
            }
            Some(TapPayload::Notification(Notification::NotMatched(not_matched)))
                if not_matched.pattern == pattern_not_matched =>
            {
                continue
            }
            _ => panic!("unexpected payload"),
        }
    }

    // Send some events down the wire. Waiting until the first notifications are in
    // to ensure the event handler has been initialized.
    let log_event = LogEvent::default();
    let metric_event = Metric::new(
        id.to_string(),
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );

    fanout
        .send(vec![metric_event].into(), None)
        .await
        .expect("should not fail");
    fanout
        .send(vec![log_event].into(), None)
        .await
        .expect("should not fail");

    // 3rd payload should be the metric event
    assert!(matches!(
        sink_rx.recv().await,
        Some(TapPayload::Metric(output, _)) if output.output_id == id
    ));

    // 4th payload should be the log event
    assert!(matches!(
        sink_rx.recv().await,
        Some(TapPayload::Log(output, _)) if output.output_id == id
    ));
}

fn assert_notification(payload: OutputEventsPayload) -> Notification {
    if let OutputEventsPayload::Notification(event_notification) = payload {
        event_notification.notification
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
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
            count: 200,
            format: OutputFormat::Json,
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["in"],
        BlackholeConfig {
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let source_tap_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(HashSet::from(["in".to_string()]), HashSet::new()),
        500,
        100,
    );

    let source_tap_events: Vec<_> = source_tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(source_tap_events[0][0].clone()),
        Notification::Matched(Matched::new("in".to_string()))
    );
    let _log = assert_log(source_tap_events[1][0].clone());
}

#[tokio::test]
async fn integration_test_source_metric() {
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
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
            metrics: Some(vec![MetricConfig {
                field: "message".try_into().expect("Fixed template string"),
                name: None,
                namespace: None,
                tags: None,
                metric: MetricTypeConfig::Gauge,
            }]),
            all_metrics: None,
        },
    );
    config.add_sink(
        "out",
        &["to_metric"],
        BlackholeConfig {
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let source_tap_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(HashSet::from(["to_metric".to_string()]), HashSet::new()),
        500,
        100,
    );

    let source_tap_events: Vec<_> = source_tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(source_tap_events[0][0].clone()),
        Notification::Matched(Matched::new("to_metric".to_string()))
    );
    assert_metric(source_tap_events[1][0].clone());
}

#[tokio::test]
async fn integration_test_transform() {
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
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
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let transform_tap_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(HashSet::from(["transform".to_string()]), HashSet::new()),
        500,
        100,
    );

    let transform_tap_events: Vec<_> = transform_tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(transform_tap_events[0][0].clone()),
        Notification::Matched(Matched::new("transform".to_string()))
    );
    let _log = assert_log(transform_tap_events[1][0].clone());
}

#[tokio::test]
async fn integration_test_transform_input() {
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
            count: 200,
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: vec!["test".to_string()],
            },
            ..Default::default()
        },
    );
    config.add_transform(
        "transform",
        &["in"],
        RemapConfig {
            source: Some(".message = \"new message\"".to_string()),
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["in"],
        BlackholeConfig {
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let tap_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(
            HashSet::new(),
            HashSet::from(["transform".to_string(), "in".to_string()]),
        ),
        500,
        100,
    );

    let tap_events: Vec<_> = tap_stream.take(4).collect().await;

    let notifications = [
        assert_notification(tap_events[0][0].clone()),
        assert_notification(tap_events[1][0].clone()),
        assert_notification(tap_events[2][0].clone()),
    ];
    assert!(notifications
        .iter()
        .any(|n| *n == Notification::Matched(Matched::new("transform".to_string()))));
    // "in" is not matched since it corresponds to a source
    assert!(notifications
        .iter()
        .any(|n| *n == Notification::NotMatched(NotMatched::new("in".to_string()))));
    // "in" generates an invalid match notification to warn against an
    // attempt to tap the input of a source
    assert!(notifications.iter().any(|n| *n
            == Notification::InvalidMatch(InvalidMatch::new("[tap] Warning: source inputs cannot be tapped. Input pattern 'in' matches sources [\"in\"]".to_string(), "in".to_string(), vec!["in".to_string()]))));

    assert_eq!(
        assert_log(tap_events[3][0].clone())
            .get_message()
            .unwrap_or_default(),
        "test"
    );
}

#[tokio::test]
async fn integration_test_sink() {
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
            count: 200,
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: vec!["test".to_string()],
            },
            ..Default::default()
        },
    );
    config.add_transform(
        "transform",
        &["in"],
        RemapConfig {
            source: Some(".message = \"new message\"".to_string()),
            ..Default::default()
        },
    );
    config.add_sink(
        "out",
        &["transform"],
        BlackholeConfig {
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let tap_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(HashSet::new(), HashSet::from(["out".to_string()])),
        500,
        100,
    );

    let tap_events: Vec<_> = tap_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(tap_events[0][0].clone()),
        Notification::Matched(Matched::new("out".to_string()))
    );
    assert_eq!(
        assert_log(tap_events[1][0].clone())
            .get_message()
            .unwrap_or_default(),
        "new message"
    );
}

#[tokio::test]
async fn integration_test_tap_non_default_output() {
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
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
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let transform_tap_remap_dropped_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(
            HashSet::from(["transform.dropped".to_string()]),
            HashSet::new(),
        ),
        500,
        100,
    );

    let transform_tap_events: Vec<_> = transform_tap_remap_dropped_stream.take(2).collect().await;

    assert_eq!(
        assert_notification(transform_tap_events[0][0].clone()),
        Notification::Matched(Matched::new("transform.dropped".to_string()))
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
    trace_init();

    let mut config = Config::builder();
    config.add_source(
        "in-test1",
        DemoLogsConfig {
            interval: Duration::from_secs_f64(0.01),
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
            interval: Duration::from_secs_f64(0.01),
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
            print_interval_secs: Duration::from_secs(1),
            rate: None,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let mut transform_tap_all_outputs_stream = create_events_stream(
        topology.watch(),
        TapPatterns::new(HashSet::from(["transform*".to_string()]), HashSet::new()),
        500,
        100,
    );

    let transform_tap_notifications = transform_tap_all_outputs_stream.next().await.unwrap();
    assert_eq!(
        assert_notification(transform_tap_notifications[0].clone()),
        Notification::Matched(Matched::new("transform*".to_string()))
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
