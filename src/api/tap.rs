use super::{ControlMessage, ControlSender};
use crate::{
    event::{Event, LogEvent},
    topology::fanout::RouterSink,
};
use futures::{channel::mpsc, SinkExt, StreamExt};
use uuid::Uuid;

type TapSender = mpsc::UnboundedSender<TapResult>;

pub enum TapError {
    ComponentInvalid,
    ComponentGoneAway,
}

pub enum TapResult {
    LogEvent(String, LogEvent),
    Error(String, TapError),
}

impl TapResult {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_, _))
    }
}

pub enum TapControl {
    Start(TapSink),
    Stop(TapSink),
}

#[derive(Clone)]
pub struct TapSink {
    id: Uuid,
    input_name: String,
    tap_tx: TapSender,
}

impl TapSink {
    /// Creates a new tap sink, and spawn a listener per sink
    pub fn new(input_name: &str, tap_tx: TapSender) -> Self {
        Self {
            id: Uuid::new_v4(),
            input_name: input_name.to_string(),
            tap_tx,
        }
    }

    pub fn router(&self) -> RouterSink {
        let (event_tx, mut event_rx) = mpsc::unbounded();

        let input_name = self.input_name.clone();
        let mut tap_tx = self.tap_tx.clone();

        tokio::spawn(async move {
            while let Some(ev) = event_rx.next().await {
                if let Event::Log(ev) = ev {
                    let _ = tap_tx.start_send(TapResult::LogEvent(input_name.clone(), ev));
                }
            }
        });

        Box::new(event_tx.sink_map_err(|_| ()))
    }

    pub fn name(&self) -> String {
        self.id.to_string()
    }

    pub fn input_name(&self) -> String {
        self.input_name.clone()
    }

    pub fn start(&self) -> TapControl {
        TapControl::Start(self.clone())
    }

    pub fn stop(&self) -> TapControl {
        TapControl::Stop(self.clone())
    }

    pub fn component_invalid(&mut self) {
        let _ = self.tap_tx.start_send(TapResult::Error(
            self.input_name.clone(),
            TapError::ComponentInvalid,
        ));
    }

    pub fn component_gone_away(&mut self) {
        let _ = self.tap_tx.start_send(TapResult::Error(
            self.input_name.clone(),
            TapError::ComponentGoneAway,
        ));
    }
}

pub struct TapController {
    control_tx: ControlSender,
    sink: TapSink,
}

impl TapController {
    pub fn new(control_tx: ControlSender, sink: TapSink) -> Self {
        let _ = control_tx.send(ControlMessage::Tap(sink.start()));
        Self { control_tx, sink }
    }
}

impl Drop for TapController {
    fn drop(&mut self) {
        let _ = self.control_tx.send(ControlMessage::Tap(self.sink.stop()));
    }
}
