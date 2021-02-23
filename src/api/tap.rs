use super::{ControlMessage, ControlSender};
use crate::{
    event::{Event, LogEvent},
    topology::fanout::RouterSink,
};
use futures::{channel::mpsc, SinkExt, StreamExt};
use uuid::Uuid;

type EventSender = mpsc::UnboundedSender<Event>;
type LogEventSender = mpsc::UnboundedSender<LogEvent>;

pub enum TapControl {
    Start(TapSink),
    Stop(TapSink),
}

#[derive(Clone)]
pub struct TapSink {
    id: Uuid,
    input_name: String,
    event_tx: EventSender,
}

impl TapSink {
    /// Creates a new tap sink, and spawn a listener per sink
    pub fn new(input_name: &str, mut log_event_tx: LogEventSender) -> Self {
        let (event_tx, mut event_rx) = mpsc::unbounded();

        tokio::spawn(async move {
            while let Some(ev) = event_rx.next().await {
                if let Event::Log(ev) = ev {
                    let _ = log_event_tx.start_send(ev);
                }
            }
        });

        Self {
            id: Uuid::new_v4(),
            input_name: input_name.to_string(),
            event_tx,
        }
    }

    pub fn router(&self) -> RouterSink {
        Box::new(self.event_tx.clone().sink_map_err(|_| ()))
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
