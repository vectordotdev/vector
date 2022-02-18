use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::try_join_all, FutureExt, Sink};
use itertools::Itertools;
use tokio::sync::{
    mpsc as tokio_mpsc,
    mpsc::error::{SendError, TrySendError},
    oneshot,
};
use uuid::Uuid;
use vector_core::event::Metric;

use super::{ShutdownRx, ShutdownTx};
use crate::{
    config::{ComponentKey, OutputId},
    event::{Event, LogEvent, TraceEvent},
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
    Metric(OutputId, Metric),
    Notification(String, TapNotification),
    Trace(OutputId, TraceEvent),
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
}

impl TapSink {
    pub const fn new(tap_tx: TapSender, output_id: OutputId) -> Self {
        Self { tap_tx, output_id }
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
            Event::Log(log) => TapPayload::Log(self.output_id.clone(), log),
            Event::Metric(metric) => TapPayload::Metric(self.output_id.clone(), metric),
            Event::Trace(trace) => TapPayload::Trace(self.output_id.clone(), trace),
        };

        if let Err(TrySendError::Closed(payload)) = self.tap_tx.try_send(payload) {
            debug!(
                message = "Couldn't send event.",
                payload = ?payload,
                component_id = ?self.output_id,
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
                for (output_id,  control_tx) in outputs.iter() {
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
