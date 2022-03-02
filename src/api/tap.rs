use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::try_join_all, stream, FutureExt, Sink, SinkExt};
use itertools::Itertools;
use tokio::sync::{
    mpsc as tokio_mpsc,
    mpsc::error::{SendError, TrySendError},
    oneshot,
};
use uuid::Uuid;
use vector_core::event::Metric;

use super::{schema::events::TapPatterns, ShutdownRx, ShutdownTx};
use crate::{
    config::ComponentKey,
    event::{Event, EventArray, EventContainer, LogEvent, TraceEvent},
    topology::{fanout, fanout::ControlChannel, TapOutput, TapResource, WatchRx},
};

/// A tap sender is the control channel used to surface tap payloads to a client.
type TapSender = tokio_mpsc::Sender<TapPayload>;

/// Clients can supply glob patterns to find matched topology components.
trait GlobMatcher<T> {
    fn matches_glob(&self, rhs: T) -> bool;
}

impl GlobMatcher<&str> for String {
    fn matches_glob(&self, rhs: &str) -> bool {
        match glob::Pattern::new(self) {
            Ok(pattern) => pattern.matches(rhs),
            _ => false,
        }
    }
}

/// Distinguishing between pattern variants helps us preserve user-friendly tap
/// notifications. Otherwise, after translating an input pattern into relevant
/// output patterns, we'd be unable to send a [`TapPayload::Notification`] with
/// the original user-specified input pattern.
#[derive(Debug, Eq, PartialEq, Hash)]
enum Pattern {
    /// A pattern used to tap into outputs of components
    OutputPattern(String),
    /// A pattern used to tap into inputs of components.
    ///
    /// For a tap user, an input pattern is effectively a shortcut for specifying
    /// one or more output patterns since a component's inputs are other
    /// components' outputs. This variant captures the original user-supplied
    /// pattern alongside the output patterns it's translated into.
    InputPattern(String, Vec<String>),
}

impl GlobMatcher<&str> for Pattern {
    fn matches_glob(&self, rhs: &str) -> bool {
        match self {
            Pattern::OutputPattern(pattern) => pattern.matches_glob(rhs),
            Pattern::InputPattern(_, patterns) => {
                patterns.iter().any(|pattern| pattern.matches_glob(rhs))
            }
        }
    }
}

/// A tap notification signals whether a pattern matches a component.
#[derive(Debug)]
pub enum TapNotification {
    Matched,
    NotMatched,
}

/// A tap payload contains events or notifications that alert users about the
/// status of the tap request.
#[derive(Debug)]
pub enum TapPayload {
    Log(TapOutput, LogEvent),
    Metric(TapOutput, Metric),
    Notification(String, TapNotification),
    Trace(TapOutput, TraceEvent),
}

impl TapPayload {
    /// Raise a `matched` event against the provided pattern.
    pub fn matched<T: Into<String>>(pattern: T) -> Self {
        Self::Notification(pattern.into(), TapNotification::Matched)
    }

    /// Raise a `not_matched` event against the provided pattern.
    pub fn not_matched<T: Into<String>>(pattern: T) -> Self {
        Self::Notification(pattern.into(), TapNotification::NotMatched)
    }
}

/// A `TapSink` is used as an output channel for a topology component, and receives
/// `Event`s.
pub struct TapSink {
    tap_tx: TapSender,
    output: TapOutput,
}

impl TapSink {
    pub const fn new(tap_tx: TapSender, output: TapOutput) -> Self {
        Self { tap_tx, output }
    }
}

impl Sink<Event> for TapSink {
    type Error = ();

    /// This sink is always ready to accept, because TapSink should never cause back-pressure.
    /// Events will be dropped instead of propagating back-pressure
    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Immediately send the event to the tap_tx, only if it has room. Otherwise just drop it
    fn start_send(self: Pin<&mut Self>, event: Event) -> Result<(), Self::Error> {
        let payload = match event {
            Event::Log(log) => TapPayload::Log(self.output.clone(), log),
            Event::Metric(metric) => TapPayload::Metric(self.output.clone(), metric),
            Event::Trace(trace) => TapPayload::Trace(self.output.clone(), trace),
        };

        if let Err(TrySendError::Closed(payload)) = self.tap_tx.try_send(payload) {
            debug!(
                message = "Couldn't send event.",
                payload = ?payload,
                component_id = ?self.output.output_id,
            );
        }

        Ok(())
    }

    /// Events are immediately flushed, so this doesn't do anything
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

/// A tap sink spawns a process for listening for topology changes. If topology changes,
/// sinks are rewired to accommodate matched/unmatched patterns.
#[derive(Debug)]
pub struct TapController {
    _shutdown: ShutdownTx,
}

impl TapController {
    /// Creates a new tap sink, and spawns a handler for watching for topology changes
    /// and a separate inner handler for events. Uses a oneshot channel to trigger shutdown
    /// of handlers when the `TapSink` drops out of scope.
    pub fn new(watch_rx: WatchRx, tap_tx: TapSender, patterns: TapPatterns) -> Self {
        let (_shutdown, shutdown_rx) = oneshot::channel();

        tokio::spawn(tap_handler(patterns, tap_tx, watch_rx, shutdown_rx));

        Self { _shutdown }
    }
}

/// Provides a `ShutdownTx` that disconnects a component sink when it drops out of scope.
fn shutdown_trigger(control_tx: ControlChannel, sink_id: ComponentKey) -> ShutdownTx {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        let _ = shutdown_rx.await;
        if control_tx
            .send(fanout::ControlMessage::Remove(sink_id.clone()))
            .is_err()
        {
            debug!(message = "Couldn't disconnect sink.", ?sink_id);
        } else {
            debug!(message = "Disconnected sink.", ?sink_id);
        }
    });

    shutdown_tx
}

/// Sends a 'matched' tap payload.
async fn send_matched(tx: TapSender, pattern: String) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending matched notification.", pattern = ?pattern);
    tx.send(TapPayload::matched(pattern)).await
}

/// Sends a 'not matched' tap payload.
async fn send_not_matched(tx: TapSender, pattern: String) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending not matched notification.", pattern = ?pattern);
    tx.send(TapPayload::not_matched(pattern)).await
}

/// Returns a tap handler that listens for topology changes, and connects sinks to observe
/// `LogEvent`s` when a component matches one or more of the provided patterns.
async fn tap_handler(
    patterns: TapPatterns,
    tx: TapSender,
    mut watch_rx: WatchRx,
    mut shutdown_rx: ShutdownRx,
) {
    debug!(message = "Started tap.", outputs_patterns = ?patterns.for_outputs, inputs_patterns = ?patterns.for_inputs);

    // Sinks register for the current tap. Contains the id of the matched component, and
    // a shutdown trigger for sending a remove control message when matching sinks change.
    let mut sinks: HashMap<ComponentKey, _> = HashMap::new();

    // Recording user-provided patterns for later use in sending notifications
    // (determining patterns which did not match)
    let user_provided_patterns = patterns.all_patterns();

    // The patterns that matched on the last iteration, to compare with the latest
    // round of matches when sending notifications.
    let mut last_matches = HashSet::new();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            Ok(_) = watch_rx.changed() => {
                // Cache of matched patterns. A `HashSet` is used here to ignore repetition.
                let mut matched = HashSet::new();

                // Borrow and clone the latest resources to register sinks. Since this blocks the
                // watch channel and the returned ref isn't `Send`, this requires a clone.
                let TapResource {
                    outputs,
                    inputs,
                    removals,
                } = watch_rx.borrow().clone();

                // Remove tap sinks from components that have gone away/can no longer match.
                let updated_keys = outputs.keys().map(|output| output.output_id.component.clone()).collect::<HashSet<_>>();
                sinks.retain(|key, _| {
                    !removals.contains(key) && updated_keys.contains(key) || {
                        debug!(message = "Removing component.", component_id = %key);
                        false
                    }
                });

                let mut component_id_patterns = patterns.for_outputs.iter().cloned().map(Pattern::OutputPattern).collect::<HashSet<_>>();

                // Matching an input pattern is equivalent to matching the outputs of the component's inputs
                for pattern in patterns.for_inputs.iter() {
                    match inputs.iter().filter(|(key, _)|
                        pattern.matches_glob(&key.to_string())
                    ).flat_map(|(_, related_inputs)| related_inputs.iter().map(|id| id.to_string()).collect_vec()).collect::<HashSet<_>>() {
                        found if !found.is_empty() => {
                            component_id_patterns.insert(Pattern::InputPattern(pattern.clone(), found.into_iter().collect_vec()));
                        }
                        _ => {
                            debug!(message="Input pattern not expanded: no matching components.", ?pattern);
                        }
                    }
                }

                // Loop over all outputs, and connect sinks for the components that match one
                // or more patterns.
                for (output, control_tx) in outputs.iter() {
                    match component_id_patterns
                        .iter()
                        .filter(|pattern| pattern.matches_glob(&output.output_id.to_string()))
                        .collect_vec()
                    {
                        found if !found.is_empty() => {
                            debug!(
                                message="Component matched.",
                                ?output.output_id, ?component_id_patterns, matched = ?found
                            );

                            // (Re)connect the sink. This is necessary because a sink may be
                            // reconfigured with the same id as a previous, and we are not
                            // getting involved in config diffing at this point.
                            let sink_id = Uuid::new_v4().to_string();
                            let sink = TapSink::new(tx.clone(), output.clone())
                                .with_flat_map(|events: EventArray| stream::iter(events.into_events().map(Ok)));

                            // Attempt to connect the sink.
                            match control_tx
                                .send(fanout::ControlMessage::Add(ComponentKey::from(sink_id.as_str()), Box::pin(sink)))
                            {
                                Ok(_) => {
                                    debug!(
                                        message = "Sink connected.", ?sink_id, ?output.output_id,
                                    );

                                    // Create a sink shutdown trigger to remove the sink
                                    // when matched components change.
                                    sinks.entry(output.output_id.component.clone()).or_insert_with(Vec::new).push(
                                        shutdown_trigger(control_tx.clone(), ComponentKey::from(sink_id.as_str()))
                                    );
                                }
                                Err(error) => {
                                    error!(
                                        message = "Couldn't connect sink.",
                                        ?error,
                                        ?output.output_id,
                                        ?sink_id,
                                    );
                                }
                            }

                            matched.extend(found.iter().map(|pattern| {
                                match pattern {
                                    Pattern::OutputPattern(p) => p.to_owned(),
                                    Pattern::InputPattern(p, _) => p.to_owned(),
                                }
                            }));
                        }
                        _ => {
                            debug!(
                                message="Component not matched.", ?output.output_id, ?component_id_patterns
                            );
                        }
                    }
                }

                // Send notifications to the client. The # of notifications will always be
                // exactly equal to the number of patterns, so we can pre-allocate capacity.
                let mut notifications = Vec::with_capacity(component_id_patterns.len());

                // Matched notifications.
                for pattern in matched.difference(&last_matches) {
                    notifications.push(send_matched(tx.clone(), pattern.clone()).boxed());
                }

                // Not matched notifications.
                for pattern in user_provided_patterns.difference(&matched) {
                    notifications.push(send_not_matched(tx.clone(), pattern.clone()).boxed());
                }

                last_matches = matched;

                // Send all events. If any event returns an error, this means the client
                // channel has gone away, so we can break the loop.
                if try_join_all(notifications).await.is_err() {
                    debug!("Couldn't send notification(s); tap gone away.");
                    break;
                }
            }
        }
    }

    debug!(message = "Stopped tap.", outputs_patterns = ?patterns.for_outputs, inputs_patterns = ?patterns.for_inputs);
}

#[cfg(all(
    test,
    feature = "sinks-blackhole",
    feature = "sources-demo_logs",
    feature = "transforms-log_to_metric",
    feature = "transforms-remap",
))]
mod tests {
    use crate::api::schema::events::{create_events_stream, log, metric};
    use crate::config::{Config, OutputId};
    use crate::transforms::log_to_metric::{GaugeConfig, LogToMetricConfig, MetricConfig};
    use futures::SinkExt;
    use tokio::sync::watch;

    use super::*;
    use crate::api::schema::events::notification::{EventNotification, EventNotificationType};
    use crate::api::schema::events::output::OutputEventsPayload;
    use crate::event::{Metric, MetricKind, MetricValue};
    use crate::sinks::blackhole::BlackholeConfig;
    use crate::sources::demo_logs::{DemoLogsConfig, OutputFormat};
    use crate::test_util::start_topology;
    use crate::transforms::remap::RemapConfig;
    use futures::StreamExt;

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
            removals: HashSet::new(),
        };

        let (watch_tx, watch_rx) = watch::channel(TapResource::default());
        let (sink_tx, mut sink_rx) = tokio_mpsc::channel(10);

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
        let log_event = LogEvent::default();
        let metric_event = Metric::new(
            id.to_string(),
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );

        let _ = fanout.send(vec![metric_event].into()).await.unwrap();
        let _ = fanout.send(vec![log_event].into()).await.unwrap();

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
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let source_tap_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(HashSet::from(["in".to_string()]), HashSet::new()),
            500,
            100,
        );

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
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let source_tap_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(HashSet::from(["to_metric".to_string()]), HashSet::new()),
            500,
            100,
        );

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
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let transform_tap_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(HashSet::from(["transform".to_string()]), HashSet::new()),
            500,
            100,
        );

        let transform_tap_events: Vec<_> = transform_tap_stream.take(2).collect().await;

        assert_eq!(
            assert_notification(transform_tap_events[0][0].clone()),
            EventNotification::new("transform".to_string(), EventNotificationType::Matched)
        );
        let _log = assert_log(transform_tap_events[1][0].clone());
    }

    #[tokio::test]
    async fn integration_test_transform_input() {
        let mut config = Config::builder();
        config.add_source(
            "in",
            DemoLogsConfig {
                interval: 0.01,
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
                print_interval_secs: 1,
                rate: None,
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let tap_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(
                HashSet::new(),
                HashSet::from(["transform".to_string(), "in".to_string()]),
            ),
            500,
            100,
        );

        let tap_events: Vec<_> = tap_stream.take(3).collect().await;

        let notifications = [
            assert_notification(tap_events[0][0].clone()),
            assert_notification(tap_events[1][0].clone()),
        ];
        assert!(notifications.iter().any(|n| *n
            == EventNotification::new("transform".to_string(), EventNotificationType::Matched)));
        // "in" is not matched since it corresponds to a source
        assert!(notifications
            .iter()
            .any(|n| *n
                == EventNotification::new("in".to_string(), EventNotificationType::NotMatched)));

        assert_eq!(
            assert_log(tap_events[2][0].clone())
                .get_message()
                .unwrap_or_default(),
            "test"
        );
    }

    #[tokio::test]
    async fn integration_test_sink() {
        let mut config = Config::builder();
        config.add_source(
            "in",
            DemoLogsConfig {
                interval: 0.01,
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
                print_interval_secs: 1,
                rate: None,
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let tap_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(HashSet::new(), HashSet::from(["out".to_string()])),
            500,
            100,
        );

        let tap_events: Vec<_> = tap_stream.take(2).collect().await;

        assert_eq!(
            assert_notification(tap_events[0][0].clone()),
            EventNotification::new("out".to_string(), EventNotificationType::Matched)
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
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let transform_tap_remap_dropped_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(
                HashSet::from(["transform.dropped".to_string()]),
                HashSet::new(),
            ),
            500,
            100,
        );

        let transform_tap_events: Vec<_> =
            transform_tap_remap_dropped_stream.take(2).collect().await;

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
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

        let mut transform_tap_all_outputs_stream = create_events_stream(
            topology.watch(),
            TapPatterns::new(HashSet::from(["transform*".to_string()]), HashSet::new()),
            500,
            100,
        );

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
}
