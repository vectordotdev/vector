use super::{ShutdownRx, ShutdownTx};
use crate::{
    event::{Event, LogEvent},
    topology::{fanout, WatchRx},
};
use futures::{channel::mpsc as futures_mpsc, future::try_join_all, FutureExt, SinkExt, StreamExt};
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
};
use tokio::sync::{mpsc as tokio_mpsc, mpsc::error::SendError, oneshot};
use uuid::Uuid;

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

/// A tap notification signals whether a pattern matches a component.
#[derive(Debug)]
pub enum TapNotification {
    Matched,
    NotMatched,
}

/// A tap payload can either contain a log event or a notification that's intended
/// to be communicated back to the client to alert them about the status of the tap request.
#[derive(Debug)]
pub enum TapPayload {
    LogEvent(String, LogEvent),
    Notification(String, TapNotification),
}

impl TapPayload {
    pub fn matched(input_name: &str) -> Self {
        Self::Notification(input_name.to_string(), TapNotification::Matched)
    }

    pub fn not_matched(input_name: &str) -> Self {
        Self::Notification(input_name.to_string(), TapNotification::NotMatched)
    }
}

/// A tap sink spawns a process for listening for topology changes. If topology changes,
/// sinks are rewired to accommodate matched/unmatched patterns.
#[derive(Debug)]
pub struct TapSink {
    _shutdown: ShutdownTx,
}

impl TapSink {
    /// Creates a new tap sink, and spawns a handler for watching for topology changes
    /// and a separate inner handler for events. Uses a oneshot channel to trigger shutdown
    /// of handlers when the `TapSink` drops out of scope.
    pub fn new(watch_rx: WatchRx, tap_tx: TapSender, patterns: &[String]) -> Self {
        let (_shutdown, shutdown_rx) = oneshot::channel();

        tokio::spawn(tap_handler(
            patterns.iter().cloned().collect(),
            tap_tx,
            watch_rx,
            shutdown_rx,
        ));

        Self { _shutdown }
    }
}

/// Sends a 'matched' tap payload.
async fn send_matched(mut tx: TapSender, pattern: &str) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending matched notification.", pattern = ?pattern);
    tx.send(TapPayload::matched(pattern)).await
}

/// Sends a 'not matched' tap payload.
async fn send_not_matched(mut tx: TapSender, pattern: &str) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending not matched notification.", pattern = ?pattern);
    tx.send(TapPayload::not_matched(pattern)).await
}

/// Makes a `RouterSink` that relays `LogEvent` as `TapPayload::LogEvent` to a client.
fn make_router(mut tx: TapSender, component_name: &str) -> fanout::RouterSink {
    let (event_tx, mut event_rx) = futures_mpsc::unbounded();
    let component_name = component_name.to_string();

    tokio::spawn(async move {
        debug!(message = "Spawned event handler.", component_name = ?component_name);

        while let Some(ev) = event_rx.next().await {
            if let Event::Log(ev) = ev {
                if let Err(err) = tx
                    .send(TapPayload::LogEvent(component_name.clone(), ev))
                    .await
                {
                    debug!(
                        message = "Couldn't send log event.",
                        error = ?err,
                        component_name = ?component_name);
                    break;
                }
            }
        }

        debug!(message = "Stopped event handler.", component_name = ?component_name);
    });

    Box::new(event_tx.sink_map_err(|_| ()))
}

/// Returns a tap handler that listens for topology changes, and connects sinks to observe
/// `LogEvent`s` when a component matches one or more of the provided patterns.
async fn tap_handler(
    patterns: HashSet<String>,
    tx: TapSender,
    mut watch_rx: WatchRx,
    mut shutdown_rx: ShutdownRx,
) {
    debug!(message = "Started tap.", patterns = ?patterns);

    // Sinks register for the current tap. Will be updated as new components match.
    let mut sinks = HashMap::new();

    // Keep a copy of the last topology snapshot, for later clean-up.
    let mut last_outputs = None;

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            Some(outputs) = watch_rx.recv() => {
                // Get the patterns that matched on the last iteration, to compare with the latest
                // round of matches when sending notifications.
                let last_matches = patterns
                    .iter()
                    .filter(|pattern| sinks.keys().any(|name: &String| pattern.matches_glob(name)))
                    .collect::<HashSet<_>>();

                // Iterate over outputs, returning a set of matched patterns from this latest round.
                let matched = outputs
                    .iter()
                    .filter_map(|(name, control_tx)| {
                        match patterns
                            .iter()
                            .filter(|pattern| pattern.matches_glob(name))
                            .collect_vec()
                        {
                            matched if !matched.is_empty() => {
                                debug!(
                                    message="Component matched.",
                                    component_name = ?name, patterns = ?patterns, matched = ?matched
                                );

                                // (Re)connect the sink. This is necessary because a sink may be
                                // reconfigured with the same name as a previous, and we are not
                                // getting involved in config diffing at this point.
                                let id = Uuid::new_v4().to_string();
                                let sink = make_router(tx.clone(), name);

                                match control_tx.send(fanout::ControlMessage::Add(id.to_string(), sink)) {
                                    Ok(_) => {
                                        // (Over)write the sink entry.
                                        debug!(
                                            message = "Component connected.",
                                            component_name = ?name, id = ?id
                                        );
                                        sinks.insert(name.to_string(), id);
                                    }
                                    Err(err) => {
                                        error!(
                                            message = "Couldn't connect component.",
                                            error = ?err,
                                            component_name = ?name, id = ?id
                                        );
                                    }
                                }

                                Some(matched)
                            }
                            _ => {
                                debug!(
                                    message="Component not matched.",
                                    component_name = ?name, patterns = ?patterns
                                );
                                None
                            }
                        }
                    })
                    .flatten()
                    .collect::<HashSet<_>>();

                // Remove components that have gone away.
                sinks.retain(|name, _| {
                    outputs.contains_key(name) || {
                        debug!(message = "Removing component.", component_name = ?name);
                        false
                    }
                });

                // Keep the outputs for later clean-up when a shutdown is triggered.
                last_outputs = Some(outputs);

                // Send notifications to the client. The # of notifications will always be
                // exactly equal to the number of patterns, so we can pre-allocate capacity.
                let mut notifications = Vec::with_capacity(patterns.len());

                // Matched notifications.
                for pattern in matched.difference(&last_matches) {
                    notifications.push(send_matched(tx.clone(), pattern).boxed());
                }

                // Not matched notifications.
                for pattern in HashSet::from_iter(&patterns).difference(&matched) {
                    notifications.push(send_not_matched(tx.clone(), pattern).boxed());
                }

                // Send all events. If any event returns an error, this means the client
                // channel has gone away, so we can break out the loop.
                if try_join_all(notifications).await.is_err() {
                    debug!("Couldn't send notification(s); tap gone away.");
                    break;
                }
            }
        }
    }

    // At this point, the tap handler is being shut down due to the client/subscription
    // going away. Clean up tap sinks by disconnecting them from the components being observed.
    if let Some(outputs) = last_outputs {
        for (name, id) in sinks {
            if let Some(control_tx) = outputs.get(&name) {
                if let Err(err) = control_tx.send(fanout::ControlMessage::Remove(id)) {
                    error!(
                        message = "Couldn't disconnect tap sink.",
                        error = ?err,
                        component_name = ?name);
                }
            }
        }
    }

    debug!(message = "Stopped tap.", patterns = ?patterns);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::event::{Metric, MetricKind, MetricValue};
    use tokio::sync::watch;

    #[test]
    /// Patterns should accept globbing.
    fn matches() {
        let patterns = ["ab*", "12?", "xy?"];

        // Should find.
        for name in &["abc", "123", "xyz"] {
            assert!(patterns.iter().any(|p| p.to_string().matches_glob(name)));
        }

        // Should not find.
        for name in &["xzy", "ad*", "1234"] {
            assert!(!patterns.iter().any(|p| p.to_string().matches_glob(name)));
        }
    }

    #[tokio::test]
    /// A tap sink should match a pattern, receive the correct notifications, and
    /// discard non `LogEvent` events.
    async fn sink_log_events() {
        let pattern_matched = "tes*";
        let pattern_not_matched = "xyz";
        let name = "test";

        let (mut fanout, control_tx) = fanout::Fanout::new();
        let mut outputs = HashMap::new();
        outputs.insert(name.to_string(), control_tx);

        let (_watch_tx, watch_rx) = watch::channel(outputs);
        let (sink_tx, mut sink_rx) = tokio_mpsc::channel(10);

        let _sink = TapSink::new(
            watch_rx,
            sink_tx,
            &[pattern_matched.to_string(), pattern_not_matched.to_string()],
        );

        // First two events should contain a notification that one pattern matched, and
        // one that didn't.
        #[allow(clippy::eval_order_dependence)]
        let notifications = vec![sink_rx.recv().await, sink_rx.recv().await];

        for notification in notifications.into_iter() {
            match notification {
                Some(TapPayload::Notification(returned_name, TapNotification::Matched))
                    if returned_name == pattern_matched =>
                {
                    continue
                }
                Some(TapPayload::Notification(returned_name, TapNotification::NotMatched))
                    if returned_name == pattern_not_matched =>
                {
                    continue
                }
                _ => panic!("unexpected payload"),
            }
        }

        // Send some events down the wire. Waiting until the first notifications are in
        // to ensure the event handler has been initialized.
        let log_event = Event::new_empty_log();
        let metric_event = Event::from(Metric::new(
            name,
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        ));

        let _ = fanout.send(metric_event).await.unwrap();
        let _ = fanout.send(log_event).await.unwrap();

        // 3rd payload should be the log event
        assert!(matches!(
            sink_rx.recv().await,
            Some(TapPayload::LogEvent(returned_name, _)) if returned_name == name
        ));
    }
}
