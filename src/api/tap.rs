use super::{ControlMessage, ControlSender};
use crate::{
    event::{Event, LogEvent},
    topology::fanout::RouterSink,
};
use futures::{channel::mpsc as futures_mpsc, SinkExt, StreamExt};
use parking_lot::RwLock;
use std::{
    collections::HashSet,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};
use tokio::sync::mpsc as tokio_mpsc;
use uuid::Uuid;

type TapSender = tokio_mpsc::Sender<TapResult>;

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

/// A tap notification signals whether a pattern matches a component
pub enum TapNotification {
    Matched,
    NotMatched,
}

/// A tap result can either contain a log event (payload), or a notification that's intended
/// to be communicated back to the client to alert them about the status of the tap request.
pub enum TapResult {
    LogEvent(String, LogEvent),
    Notification(String, TapNotification),
}

impl TapResult {
    pub fn matched(input_name: &str) -> Self {
        Self::Notification(input_name.to_string(), TapNotification::Matched)
    }

    pub fn not_matched(input_name: &str) -> Self {
        Self::Notification(input_name.to_string(), TapNotification::NotMatched)
    }
}

/// Tap control messages are used at the app-level to alert the caller when a tap has been
/// started or stopped. A 'stopped' tap request typically means that the client either terminated
/// the subscription explicitly, or the connection went away.
pub enum TapControl {
    Start(Arc<TapSink>),
    Stop(Arc<TapSink>),
}

/// A tap sink acts as a receiver of `LogEvent` data, and relays it to the connecting
/// GraphQL client.
pub struct TapSink {
    id: Uuid,
    patterns: HashSet<String>,
    sink_ids: RwLock<HashSet<Uuid>>,
    tap_tx: TapSender,
}

impl TapSink {
    pub fn new(patterns: &[String], tap_tx: TapSender) -> Self {
        let patterns = patterns.iter().cloned().collect();

        Self {
            id: Uuid::new_v4(),
            patterns,
            sink_ids: RwLock::new(HashSet::new()),
            tap_tx,
        }
    }

    /// Internal function to build a `RouterSink` from an input name. This will spawn an async
    /// task to forward on `LogEvent`s to the tap channel.
    fn make_router(&self, component_name: &str) -> RouterSink {
        let (event_tx, mut event_rx) = futures_mpsc::unbounded();
        let mut tap_tx = self.tap_tx.clone();
        let component_name = component_name.to_string();

        tokio::spawn(async move {
            while let Some(ev) = event_rx.next().await {
                if let Event::Log(ev) = ev {
                    let _ = tap_tx
                        .send(TapResult::LogEvent(component_name.clone(), ev))
                        .await;
                }
            }
        });

        Box::new(event_tx.sink_map_err(|_| ()))
    }

    /// Private convenience for sending a `TapResult` to the connected receiver.
    fn send(&self, msg: TapResult) {
        let tap_tx = self.tap_tx.clone();
        tokio::spawn(async move {
            let _ = tap_tx.clone().send(msg).await;
        });
    }

    /// Returns the pattern of inputs used to assess whether a component matches.
    pub fn patterns(&self) -> Vec<String> {
        self.patterns.iter().cloned().collect()
    }

    /// Returns a vector of sink IDs, as strings.
    pub fn sink_ids(&self) -> Vec<String> {
        self.sink_ids
            .read()
            .iter()
            .map(|uuid| uuid.to_string())
            .collect()
    }

    /// Returns true if the provided component name matches a glob pattern of the inputs that
    /// this sink is observing.
    pub fn matches(&self, component_name: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.matches_glob(component_name))
    }

    /// Returns the pattern that matches against a component name, if found.
    pub fn find_match(&self, component_name: &str) -> Option<String> {
        self.patterns
            .iter()
            .find(|pattern| pattern.matches_glob(component_name))
            .map(|pattern| pattern.to_string())
    }

    /// Returns (if it exists) a tuple of the generated sink name, and a router for handling
    /// incoming `Event`s, based on the configured input name.
    pub fn make_output(&self, component_name: &str) -> Option<(String, RouterSink)> {
        let id = Uuid::new_v4();
        let mut sink_ids = self.sink_ids.write();
        sink_ids.insert(id);
        drop(sink_ids);

        Some((id.to_string(), self.make_router(component_name)))
    }

    pub fn send_matched(&self, pattern: &str) {
        self.send(TapResult::matched(pattern))
    }

    pub fn send_not_matched(&self, pattern: &str) {
        self.send(TapResult::not_matched(pattern))
    }
}

impl Debug for TapSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

/// `Hash` is implemented for `TapSink` to limit checking to the UUID assigned to a
/// given sink.
impl Hash for TapSink {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

/// Equality on a `TapSink` is based on whether its UUID matches.
impl PartialEq for TapSink {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TapSink {}

/// A tap controller holds a `ControlSender` and a thread-safe ref to a sink, to bubble up a
/// control message to the top of the app when a sink has 'started' or 'stopped'.
pub struct TapController {
    control_tx: ControlSender,
    sink: Arc<TapSink>,
}

impl TapController {
    pub fn new(control_tx: ControlSender, sink: TapSink) -> Self {
        let sink = Arc::new(sink);

        let _ = control_tx.send(ControlMessage::Tap(TapControl::Start(Arc::clone(&sink))));
        Self { control_tx, sink }
    }
}

/// When a `TapController` goes out of scope, a control message is sent to the controller to
/// alert that a tap sink is no longer valid. This is distinct to the weak ref that topology holds
/// for the sink, as it allows explicit action to be taken at the time the subscription goes away.
impl Drop for TapController {
    fn drop(&mut self) {
        let _ = self
            .control_tx
            .send(ControlMessage::Tap(TapControl::Stop(Arc::clone(
                &self.sink,
            ))));
    }
}

#[cfg(test)]
mod tests {
    use super::{TapControl, TapController, TapResult, TapSink};

    use crate::topology::fanout::Fanout;
    use crate::{
        api::ControlMessage,
        event::{
            metric::{Metric, MetricKind, MetricValue},
            Event,
        },
    };
    use futures::SinkExt;
    use tokio::sync::mpsc;

    #[test]
    /// Sinks should generate different UUIDs, even if they share the same channel. A sink should
    /// be equal to itself, but never another sink.
    fn sink_eq() {
        let (sink_tx, _sink_rx) = mpsc::channel(10);

        let sink1 = TapSink::new(&["test".to_string()], sink_tx.clone());
        let sink2 = TapSink::new(&["test".to_string()], sink_tx);

        assert_ne!(sink1, sink2);
    }

    #[tokio::test]
    /// `TapController` should send a `TapControl::Start` followed by `TapControl::Stop` on drop.
    async fn tap_controller_signals() {
        let (sink_tx, _sink_rx) = mpsc::channel(10);
        let (tx, mut rx) = mpsc::unbounded_channel();

        let sink = TapSink::new(&["test".to_string()], sink_tx);
        let control = TapController::new(tx, sink);
        drop(control);

        assert!(matches!(
            rx.recv().await,
            Some(ControlMessage::Tap(TapControl::Start(_)))
        ));

        assert!(matches!(
            rx.recv().await,
            Some(ControlMessage::Tap(TapControl::Stop(_)))
        ));
    }

    #[tokio::test]
    /// A tap sink should discard non `LogEvent` events.
    async fn sink_log_events() {
        let (sink_tx, mut sink_rx) = mpsc::channel(10);
        let name = "test";

        let sink = TapSink::new(&[name.to_string()], sink_tx);

        let log_event = Event::new_empty_log();
        let metric_event = Event::from(Metric::new(
            "test",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        ));

        let (_, sink) = sink.make_output(name).unwrap();

        let mut fanout = Fanout::new().0;
        fanout.add(name.to_string(), sink);

        let _ = fanout.send(metric_event).await.unwrap();
        let _ = fanout.send(log_event).await.unwrap();

        assert!(matches!(
            sink_rx.recv().await,
            Some(TapResult::LogEvent(returned_name, _)) if returned_name == name
        ));
    }
}
