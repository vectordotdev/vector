use std::{
    collections::{HashMap, HashSet, VecDeque},
    iter::FromIterator,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::try_join_all, FutureExt, Sink, SinkExt};
use itertools::Itertools;
use tokio::sync::{mpsc as tokio_mpsc, mpsc::error::SendError, oneshot};
use uuid::Uuid;

use super::{ShutdownRx, ShutdownTx};
use crate::{
    config::{ComponentKey, OutputId},
    event::{Event, LogEvent},
    topology::{fanout, fanout::ControlChannel, WatchRx},
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

/// A tap notification signals whether a pattern matches a component.
#[derive(Debug)]
pub enum TapNotification {
    Matched,
    NotMatched,
}

/// A tap payload can either contain a log/metric event or a notification that's intended
/// to be communicated back to the client to alert them about the status of the tap request.
#[derive(Debug)]
pub enum TapPayload {
    Log(OutputId, LogEvent),
    Metric(OutputId, LogEvent),
    Notification(String, TapNotification),
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
/// `Event`s. If these are of type `Event::LogEvent`, they are relayed to the tap client.
pub struct TapSink {
    tap_tx: TapSender,
    output_id: OutputId,
    buffer: VecDeque<LogEvent>,
}

impl TapSink {
    pub fn new(tap_tx: TapSender, output_id: OutputId) -> Self {
        Self {
            tap_tx,
            output_id,
            // Pre-allocate space of 100 events, which matches the default `limit` typically
            // provided to a tap subscription. If there's a higher log volume, this will block
            // until the upstream event handler has processed the event. Generally, there should
            // be little upstream pressure in the processing pipeline.
            buffer: VecDeque::with_capacity(100),
        }
    }
}

impl Sink<Event> for TapSink {
    type Error = ();

    /// The sink is ready to accept events if buffer capacity hasn't been reached.
    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// If the sink is ready, and the event is of type `LogEvent`, add to the buffer.
    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
        // If we have a `LogEvent`, and space for it in the buffer, queue it.
        if let Event::Log(ev) = item {
            if self.buffer.len() < self.buffer.capacity() {
                self.buffer.push_back(ev);
            }
        }

        Ok(())
    }

    /// Flushing means FIFO dequeuing. This is an O(1) operation on the `VecDeque` buffer.
    fn poll_flush(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        // Loop over the buffer events, pulling from the front. This will terminate when
        // the buffer is empty.
        while let Some(ev) = self.buffer.pop_front() {
            // Attempt to send upstream. If the channel is closed, log and break. If it's
            // full, return pending to reattempt later.
            match self
                .tap_tx
                .try_send(TapPayload::Log(self.output_id.clone(), ev))
            {
                Err(tokio_mpsc::error::TrySendError::Closed(payload)) => {
                    debug!(
                        message = "Couldn't send log event.",
                        payload = ?payload,
                        component_id = ?self.output_id,
                    );

                    break;
                }
                Err(tokio_mpsc::error::TrySendError::Full(_)) => return Poll::Ready(Ok(())),
                _ => continue,
            }
        }

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
    pub fn new(watch_rx: WatchRx, tap_tx: TapSender, component_id_patterns: &[String]) -> Self {
        let (_shutdown, shutdown_rx) = oneshot::channel();

        tokio::spawn(tap_handler(
            component_id_patterns.iter().cloned().collect(),
            tap_tx,
            watch_rx,
            shutdown_rx,
        ));

        Self { _shutdown }
    }
}

/// Provides a `ShutdownTx` that disconnects a component sink when it drops out of scope.
fn shutdown_trigger(mut control_tx: ControlChannel, sink_id: ComponentKey) -> ShutdownTx {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        let _ = shutdown_rx.await;
        if control_tx
            .send(fanout::ControlMessage::Remove(sink_id.clone()))
            .await
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
async fn send_matched(tx: TapSender, pattern: &str) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending matched notification.", pattern = ?pattern);
    tx.send(TapPayload::matched(pattern)).await
}

/// Sends a 'not matched' tap payload.
async fn send_not_matched(tx: TapSender, pattern: &str) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending not matched notification.", pattern = ?pattern);
    tx.send(TapPayload::not_matched(pattern)).await
}

/// Returns a tap handler that listens for topology changes, and connects sinks to observe
/// `LogEvent`s` when a component matches one or more of the provided patterns.
async fn tap_handler(
    component_id_patterns: HashSet<String>,
    tx: TapSender,
    mut watch_rx: WatchRx,
    mut shutdown_rx: ShutdownRx,
) {
    debug!(message = "Started tap.", patterns = ?component_id_patterns);

    // Sinks register for the current tap. Contains the id of the matched component, and
    // a shutdown trigger for sending a remove control message when matching sinks change.
    let mut sinks: HashMap<OutputId, _> = HashMap::new();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            Ok(_) = watch_rx.changed() => {
                // Get the patterns that matched on the last iteration, to compare with the latest
                // round of matches when sending notifications.
                let last_matches = component_id_patterns
                    .iter()
                    .filter(|pattern| sinks.keys().any(|id| pattern.matches_glob(&id.to_string())))
                    .collect::<HashSet<_>>();

                // Cache of matched patterns. A `HashSet` is used here to ignore repetition.
                let mut matched = HashSet::new();

                // Borrow and clone the latest outputs to register sinks. Since this blocks the
                // watch channel and the returned ref isn't `Send`, this requires a clone.
                let outputs = watch_rx.borrow().clone();

                // Loop over all outputs, and connect sinks for the components that match one
                // or more patterns.
                for (output_id, mut control_tx) in outputs.iter() {
                    match component_id_patterns
                        .iter()
                        .filter(|pattern| pattern.matches_glob(&output_id.to_string()))
                        .collect_vec()
                    {
                        found if !found.is_empty() => {
                            debug!(
                                message="Component matched.",
                                ?output_id, ?component_id_patterns, matched = ?found
                            );

                            // (Re)connect the sink. This is necessary because a sink may be
                            // reconfigured with the same id as a previous, and we are not
                            // getting involved in config diffing at this point.
                            let sink_id = Uuid::new_v4().to_string();
                            let sink = TapSink::new(tx.clone(), output_id.clone());

                            // Attempt to connect the sink.
                            match control_tx
                                .send(fanout::ControlMessage::Add(ComponentKey::from(sink_id.as_str()), Box::pin(sink)))
                                .await
                            {
                                Ok(_) => {
                                    debug!(
                                        message = "Sink connected.", ?sink_id, ?output_id,
                                    );

                                    // Create a sink shutdown trigger to remove the sink
                                    // when matched components change.
                                    sinks
                                        .insert(output_id.clone(), shutdown_trigger(control_tx.clone(), ComponentKey::from(sink_id.as_str())));
                                }
                                Err(error) => {
                                    error!(
                                        message = "Couldn't connect sink.",
                                        ?error,
                                        ?output_id,
                                        ?sink_id,
                                    );
                                }
                            }

                            matched.extend(found);
                        }
                        _ => {
                            debug!(
                                message="Component not matched.", ?output_id, ?component_id_patterns
                            );
                        }
                    }
                }

                // Remove components that have gone away.
                sinks.retain(|id, _| {
                    outputs.contains_key(id) || {
                        debug!(message = "Removing component.", component_id = %id);
                        false
                    }
                });

                // Send notifications to the client. The # of notifications will always be
                // exactly equal to the number of patterns, so we can pre-allocate capacity.
                let mut notifications = Vec::with_capacity(component_id_patterns.len());

                // Matched notifications.
                for pattern in matched.difference(&last_matches) {
                    notifications.push(send_matched(tx.clone(), pattern).boxed());
                }

                // Not matched notifications.
                for pattern in HashSet::from_iter(&component_id_patterns).difference(&matched) {
                    notifications.push(send_not_matched(tx.clone(), pattern).boxed());
                }

                // Send all events. If any event returns an error, this means the client
                // channel has gone away, so we can break the loop.
                if try_join_all(notifications).await.is_err() {
                    debug!("Couldn't send notification(s); tap gone away.");
                    break;
                }
            }
        }
    }

    debug!(message = "Stopped tap.", patterns = ?component_id_patterns);
}

#[cfg(test)]
mod tests {
    use futures::SinkExt;
    use tokio::sync::watch;

    use super::*;
    use crate::event::{Metric, MetricKind, MetricValue};

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
    /// A tap sink should match a pattern, receive the correct notifications, and
    /// discard non `LogEvent` events.
    async fn sink_log_events() {
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
        let log_event = Event::new_empty_log();
        let metric_event = Event::from(Metric::new(
            id.to_string(),
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        ));

        let _ = fanout.send(metric_event).await.unwrap();
        let _ = fanout.send(log_event).await.unwrap();

        // 3rd payload should be the log event
        assert!(matches!(
            sink_rx.recv().await,
            Some(TapPayload::Log(returned_id, _)) if returned_id == id
        ));
    }
}
