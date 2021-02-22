use crate::event::{Event, LogEvent};
use tokio::sync::mpsc;
use uuid::Uuid;

type EventSender = mpsc::Sender<Event>;
type LogEventSender = mpsc::Sender<LogEvent>;
type LogEventReceiver = mpsc::Receiver<LogEvent>;

pub enum TapControl {
    Start(String, EventSender),
    Stop(String),
}

pub struct TapSink {
    id: Uuid,
    input_name: String,
    event_tx: EventSender,
}

impl TapSink {
    pub fn new(input_name: &str, mut log_event_tx: LogEventSender) -> Self {
        // Spawn a 'sink' to forward events -> log event listener
        let (event_tx, mut event_rx) = mpsc::channel::<Event>(100);

        tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                if let Event::Log(ev) = ev {
                    let _ = log_event_tx.send(ev);
                }
            }
        });

        Self {
            id: Uuid::new_v4(),
            input_name: input_name.to_string(),
            event_tx,
        }
    }

    pub fn subscribe(&self) -> EventSender {
        self.event_tx.clone()
    }

    pub fn name(&self) -> String {
        self.id.to_string()
    }

    pub fn input_name(&self) -> String {
        self.input_name.clone()
    }

    pub fn start(&self) -> TapControl {
        TapControl::Start(self.id.to_string(), self.event_tx.clone())
    }
}
