use super::{ControlMessage, ControlSender};
use crate::{
    event::{Event, LogEvent},
    topology::fanout::RouterSink,
};
use futures::{channel::mpsc as futures_mpsc, SinkExt, StreamExt};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};
use tokio::sync::mpsc as tokio_mpsc;
use uuid::Uuid;

type TapSender = tokio_mpsc::Sender<TapResult>;

/// A tap notification signals whether a component is matched or unmatched.
pub enum TapNotification {
    ComponentMatched,
    ComponentNotMatched,
}

/// A tap result can either contain a log event (payload), or a notification that's intended
/// to be communicated back to the client to alert them about the status of the tap request.
pub enum TapResult {
    LogEvent(String, LogEvent),
    Notification(String, TapNotification),
}

impl TapResult {
    pub fn component_matched(input_name: &str) -> Self {
        Self::Notification(input_name.to_string(), TapNotification::ComponentMatched)
    }

    pub fn component_not_matched(input_name: &str) -> Self {
        Self::Notification(input_name.to_string(), TapNotification::ComponentNotMatched)
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
    inputs: HashMap<String, Uuid>,
    tap_tx: TapSender,
}

impl TapSink {
    pub fn new(input_names: &[String], tap_tx: TapSender) -> Self {
        // Map each input name to a UUID. The string output of the UUID will be used as the
        // sink name for topology. This never changes.
        let inputs = input_names
            .iter()
            .map(|name| (name.to_string(), Uuid::new_v4()))
            .collect();

        Self {
            id: Uuid::new_v4(),
            inputs,
            tap_tx,
        }
    }

    /// Internal function to build a `RouterSink` from an input name. This will spawn an async
    /// task to forward on `LogEvent`s to the tap channel.
    fn make_router(&self, input_name: &str) -> RouterSink {
        let (event_tx, mut event_rx) = futures_mpsc::unbounded();
        let mut tap_tx = self.tap_tx.clone();
        let input_name = input_name.to_string();

        tokio::spawn(async move {
            while let Some(ev) = event_rx.next().await {
                if let Event::Log(ev) = ev {
                    let _ = tap_tx.send(TapResult::LogEvent(input_name.clone(), ev));
                }
            }
        });

        Box::new(event_tx.sink_map_err(|_| ()))
    }

    /// Private convenience for sending a `TapResult` to the connected receiver.
    fn send(&self, msg: TapResult) {
        let _ = self.tap_tx.clone().send(msg);
    }

    /// Returns the input names of the components this sink is observing as a vector of
    /// cloned strings.
    pub fn input_names(&self) -> Vec<String> {
        self.inputs.keys().cloned().collect()
    }

    /// Get a cloned `HashMap` of the inputs. The use of a UUID for the sink name is an
    /// implementation detail, so this is returned as a string to the caller to match
    /// the expectations of topology.
    pub fn inputs(&self) -> HashMap<String, String> {
        self.inputs
            .iter()
            .map(|(name, uuid)| (name.to_string(), uuid.to_string()))
            .collect()
    }

    /// Returns (if it exists) a tuple of the generated sink name, and a router for handling
    /// incoming `Event`s, based on the configured input name.
    pub fn make_output(&self, input_name: &str) -> Option<(String, RouterSink)> {
        let id = self.inputs.get(input_name)?;

        Some((id.to_string(), self.make_router(input_name)))
    }

    pub fn component_matched(&self, input_name: &str) {
        if self.inputs.contains_key(input_name) {
            self.send(TapResult::component_matched(input_name))
        }
    }

    pub fn component_not_matched(&self, input_name: &str) {
        if self.inputs.contains_key(input_name) {
            self.send(TapResult::component_not_matched(input_name))
        }
    }
}

/// `Hash` is implemented for `TapSink` to shortcut checking to the UUID assigned to a
/// given sink.
impl Hash for TapSink {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

/// Equality on a `TapSink` is based on whether its UUID matches
impl PartialEq for TapSink {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TapSink {}

/// A tap controller holds a `ControlSender` and a thread-safe res to a sink, to bubble up a
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
