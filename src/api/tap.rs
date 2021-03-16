use super::{ShutdownRx, ShutdownTx};
use crate::{
    event::{Event, LogEvent},
    topology::{fanout, WatchRx},
};
use futures::{channel::mpsc as futures_mpsc, SinkExt, StreamExt};
use itertools::Itertools;
use std::collections::HashSet;
use tokio::sync::{mpsc as tokio_mpsc, mpsc::error::SendError, oneshot};
use uuid::Uuid;

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
pub enum TapNotification {
    Matched,
    NotMatched,
}

/// A tap payload can either contain a log event (payload), or a notification that's intended
/// to be communicated back to the client to alert them about the status of the tap request.
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

/// A tap sink spawns a process for listening to topology changes, and re-wiring sinks to
/// observe `LogEvent`s that match the provided pattern.
pub struct TapSink {
    _shutdown: ShutdownTx,
}

impl TapSink {
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

/// Returns a hashset of the patterns that component names match against.
fn matched_patterns(patterns: &HashSet<String>, component_names: &[&String]) -> HashSet<String> {
    patterns
        .iter()
        .filter(|pattern| {
            component_names
                .iter()
                .any(|component_name| pattern.matches_glob(component_name))
        })
        .map(|pattern| pattern.to_string())
        .collect()
}

/// Sends a 'matched' tap payload.
async fn send_matched(tx: &mut TapSender, pattern: &str) -> Result<(), SendError<TapPayload>> {
    tx.send(TapPayload::matched(pattern)).await
}

/// Sends a 'not matched' tap payload.
async fn send_not_matched(tx: &mut TapSender, pattern: &str) -> Result<(), SendError<TapPayload>> {
    tx.send(TapPayload::not_matched(pattern)).await
}

/// Makes a `RouterSink` that relays `LogEvent` as `TapPayload::LogEvent` to a client.
fn make_router(mut tx: TapSender, component_name: &str) -> fanout::RouterSink {
    let (event_tx, mut event_rx) = futures_mpsc::unbounded();
    let component_name = component_name.to_string();

    tokio::spawn(async move {
        while let Some(ev) = event_rx.next().await {
            if let Event::Log(ev) = ev {
                let _ = tx
                    .send(TapPayload::LogEvent(component_name.clone(), ev))
                    .await;
            }
        }
    });

    Box::new(event_tx.sink_map_err(|_| ()))
}

/// Returns a tap handler that listens for topology changes, and connects sinks to observe
/// `LogEvent`s` when a component matches one of more of the provided patterns.
async fn tap_handler(
    patterns: HashSet<String>,
    mut tx: TapSender,
    mut watch_rx: WatchRx,
    mut shutdown_rx: ShutdownRx,
) {
    let mut current = HashSet::new();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            Some(outputs) = watch_rx.recv() => {
                let component_names = outputs.keys().collect_vec();
                let matched = matched_patterns(&patterns, &component_names);

                // Remove components that don't match.
                for pattern in patterns.difference(&matched) {
                    if send_not_matched(&mut tx, &pattern).await.is_err() {
                        break;
                    }
                }

                // Add new components.
                for pattern in matched.difference(&current) {
                    if send_matched(&mut tx, &pattern).await.is_err() {
                        break;
                    }
                }

                // Make a router for each.
                for component_name in component_names
                    .into_iter()
                    .filter(|name| patterns.iter().any(|p| name.matches_glob(p)))
                {
                    if let Some(output) = outputs.get(component_name) {
                        let sink = make_router(tx.clone(), component_name);
                        let _ = output.send(fanout::ControlMessage::Add(
                            Uuid::new_v4().to_string(),
                            sink,
                        ));
                    }
                }

                current = matched;
            }
        }
    }
}
