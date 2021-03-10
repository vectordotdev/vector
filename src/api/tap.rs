use super::{ControlMessage, ControlSender};
use crate::{
    config::ConfigDiff,
    event::{Event, LogEvent},
    topology::{fanout, RunningTopology},
};
use futures::{channel::mpsc as futures_mpsc, SinkExt, StreamExt};
use itertools::Itertools;
use parking_lot::RwLock;
use std::cmp::Ordering;
use std::{
    collections::HashSet,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};
use tokio::sync::mpsc as tokio_mpsc;
use uuid::Uuid;
use weak_table::WeakHashSet;

type TapSender = tokio_mpsc::Sender<TapResult>;

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

/// A tap register holds weak references to tap sinks. Taps can be attached/detached, which
/// update topology and send relevant event notifications to the client.
pub struct TapRegister {
    tap_sinks: WeakHashSet<Weak<TapSink>>,
}

impl TapRegister {
    pub fn new() -> Self {
        Self {
            tap_sinks: WeakHashSet::new(),
        }
    }

    /// Wire a tap with topology, and add it to the register.
    pub fn attach(&mut self, topology: &mut RunningTopology, tap_sink: Arc<TapSink>) {
        tap_sink.attach(topology);
        self.tap_sinks.insert(tap_sink);
    }

    /// Remove a tap from topology, and explicitly remove from the register.
    pub fn detach(&mut self, topology: &mut RunningTopology, tap_sink: Arc<TapSink>) {
        tap_sink.detach(topology);
        self.tap_sinks.remove(&tap_sink);
    }

    /// Reconnect a diff'd config with topology, for all registered tap sinks.
    pub fn reconnect(&mut self, topology: &mut RunningTopology, diff: &ConfigDiff) {
        let to_keep = diff
            .sources
            .changed_and_added()
            .chain(diff.transforms.changed_and_added())
            .collect::<Vec<_>>();

        let to_remove = diff
            .sources
            .removed_and_changed()
            .chain(diff.transforms.removed_and_changed())
            .collect::<Vec<_>>();

        self.tap_sinks
            .iter()
            .inspect(|tap_sink| {
                tap_sink
                    .all_matched_patterns(&to_remove)
                    .difference(&tap_sink.all_matched_patterns(&to_keep))
                    .for_each(|pattern| {
                        tap_sink.send_not_matched(pattern);
                    })
            })
            .cartesian_product(&to_keep)
            .filter_map(|(tap_sink, input_name)| {
                tap_sink
                    .find_match(*input_name)
                    .map(|pattern| {
                        topology.outputs.get(*input_name).map(|tx| {
                            let (sink_name, sink) = tap_sink.make_output(*input_name);
                            debug!(
                                message = "Restarting tap.",
                                id = sink_name.as_str(),
                                input = input_name.as_str()
                            );

                            let _ = tx.send(fanout::ControlMessage::Add(sink_name, sink));
                            (tap_sink, pattern)
                        })
                    })
                    .flatten()
            })
            .sorted()
            .dedup()
            .inspect(|(tap_sink, pattern)| {
                println!("Tap ID: {}, pattern: {}", tap_sink.id, pattern);
            })
            .for_each(|(tap_sink, pattern)| tap_sink.send_matched(&pattern));
    }
}

struct Test {
    id: i32,
}

impl PartialEq for Test {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Test {}

impl Default for TapRegister {
    fn default() -> Self {
        TapRegister::new()
    }
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

    /// Build a `RouterSink` from an input name. This will spawn an async task to forward
    /// on `LogEvent`s to the tap channel.
    fn make_router(&self, component_name: &str) -> fanout::RouterSink {
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

    /// Convenience for sending a `TapResult` to the connected receiver.
    fn send(&self, msg: TapResult) {
        let tap_tx = self.tap_tx.clone();
        tokio::spawn(async move {
            let _ = tap_tx.clone().send(msg).await;
        });
    }

    /// Returns the pattern of inputs used to assess whether a component matches.
    fn patterns(&self) -> HashSet<String> {
        self.patterns.iter().cloned().collect()
    }

    /// Returns a vector of sink IDs as strings.
    fn sink_ids(&self) -> Vec<String> {
        self.sink_ids
            .read()
            .iter()
            .map(|uuid| uuid.to_string())
            .collect()
    }

    /// Returns true if the provided component name matches a glob pattern of the inputs that
    /// this sink is observing.
    fn matches(&self, component_name: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.matches_glob(component_name))
    }

    /// Returns the pattern that matches against a component name, if found.
    fn find_match(&self, component_name: &str) -> Option<String> {
        self.patterns
            .iter()
            .find(|pattern| pattern.matches_glob(component_name))
            .map(|pattern| pattern.to_string())
    }

    /// Returns a set of patterns that match the provided input.
    fn all_matched_patterns(&self, component_names: &[&String]) -> HashSet<String> {
        self.patterns
            .iter()
            .filter(|pattern| {
                component_names
                    .iter()
                    .any(|&component_name| pattern.matches_glob(component_name))
            })
            .map(|pattern| pattern.to_string())
            .collect()
    }

    /// Returns a tuple of the generated sink name, and a router for handling incoming
    /// `Event`s, based on the configured component name.
    fn make_output(&self, component_name: &str) -> (String, fanout::RouterSink) {
        let id = Uuid::new_v4();
        let mut sink_ids = self.sink_ids.write();
        sink_ids.insert(id);
        drop(sink_ids);

        (id.to_string(), self.make_router(component_name))
    }

    /// Send a 'matched' notification result.
    fn send_matched(&self, pattern: &str) {
        self.send(TapResult::matched(pattern))
    }

    /// Send a 'not matched' notification result.
    fn send_not_matched(&self, pattern: &str) {
        self.send(TapResult::not_matched(pattern))
    }

    /// Attach the current tap to running topology.
    fn attach(&self, topology: &mut RunningTopology) {
        self.patterns().iter().for_each(|pattern| {
            let found = topology
                .outputs
                .iter()
                .fold(false, |found, (component_name, tx)| {
                    if self.matches(component_name) {
                        let (sink_name, sink) = self.make_output(component_name);
                        debug!(
                            message = "Starting tap.",
                            id = sink_name.as_str(),
                            component = component_name.as_str(),
                            pattern = pattern.as_str()
                        );
                        let _ = tx.send(fanout::ControlMessage::Add(sink_name, sink));
                        true
                    } else {
                        found
                    }
                });

            // If the component pattern didn't provide any matches, alert the client.
            if !found {
                debug!(
                    message = "Waiting for matched tap component(s).",
                    pattern = pattern.as_str()
                );
                let _ = self.send_not_matched(pattern);
            }
        });
    }

    /// Detach the current tap from running topology.
    fn detach(&self, topology: &mut RunningTopology) {
        self.sink_ids().into_iter().for_each(|sink_id| {
            if let Some(tx) = topology.outputs.get(&sink_id) {
                debug!(message = "Removing tap.", id = sink_id.as_str());
                let _ = tx.send(fanout::ControlMessage::Remove(sink_id));
            }
        })
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

impl PartialOrd for TapSink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.id.cmp(&other.id))
    }
}

impl Ord for TapSink {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

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
/// alert that a tap sink is no longer valid. This is distinct to the weak ref that `TapRegister` holds
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

        let (_, sink) = sink.make_output(name);

        let mut fanout = Fanout::new().0;
        fanout.add(name.to_string(), sink);

        let _ = fanout.send(metric_event).await.unwrap();
        let _ = fanout.send(log_event).await.unwrap();

        assert!(matches!(
            sink_rx.recv().await,
            Some(TapResult::LogEvent(returned_name, _)) if returned_name == name
        ));
    }

    #[test]
    /// A configured tap sink should match glob patterns.
    fn matches() {
        let (sink_tx, _sink_rx) = mpsc::channel(10);
        let sink = TapSink::new(
            &["ab*".to_string(), "12?".to_string(), "xy?".to_string()],
            sink_tx,
        );

        // Should find.
        for pattern in &["abc", "123", "xyz"] {
            assert!(sink.matches(pattern));
        }

        // Should not find.
        for pattern in &["xzy", "ad*", "1234"] {
            assert!(!sink.matches(pattern));
        }
    }
}
