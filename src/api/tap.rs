use super::{ControlMessage, ControlSender};
use crate::{
    event::{Event, LogEvent},
    topology::fanout::RouterSink,
};
use futures::{channel::mpsc, SinkExt, StreamExt};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};
use uuid::Uuid;

type TapSender = mpsc::UnboundedSender<TapResult>;

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
        let (event_tx, mut event_rx) = mpsc::unbounded();
        let mut tap_tx = self.tap_tx.clone();
        let input_name = input_name.to_string();

        tokio::spawn(async move {
            while let Some(ev) = event_rx.next().await {
                if let Event::Log(ev) = ev {
                    let _ = tap_tx.start_send(TapResult::LogEvent(input_name.clone(), ev));
                }
            }
        });

        Box::new(event_tx.sink_map_err(|_| ()))
    }

    fn send(&self, msg: TapResult) {
        let _ = self.tap_tx.clone().start_send(msg);
    }

    pub fn input_names(&self) -> Vec<String> {
        self.inputs.keys().cloned().collect()
    }

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

impl Hash for TapSink {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl PartialEq for TapSink {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TapSink {}

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

impl Drop for TapController {
    fn drop(&mut self) {
        let _ = self
            .control_tx
            .send(ControlMessage::Tap(TapControl::Stop(Arc::clone(
                &self.sink,
            ))));
    }
}
