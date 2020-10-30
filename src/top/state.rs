use num_format::{Locale, ToFormattedString};
use std::collections::btree_map::BTreeMap;

pub static COMPONENT_HEADERS: [&str; 5] = ["Name", "Type", "Events", "Bytes", "Errors"];

pub type State = BTreeMap<String, ComponentRow>;
pub type EventTx = tokio::sync::mpsc::Sender<(String, EventType)>;
pub type EventRx = tokio::sync::mpsc::Receiver<(String, EventType)>;
pub type StateTx = tokio::sync::broadcast::Sender<State>;
pub type StateRx = tokio::sync::broadcast::Receiver<State>;

#[derive(Debug)]
pub enum EventType {
    EventsProcessedTotal(i64),
    BytesProcessedTotal(i64),
}

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub name: String,
    pub component_type: String,
    pub events_processed_total: i64,
    pub bytes_processed_total: i64,
    pub errors: i64,
}

impl ComponentRow {
    /// Helper method for formatting an i64 value -> String
    fn format_i64(val: i64) -> String {
        match val {
            0 => "--".into(),
            _ => val.to_formatted_string(&Locale::en),
        }
    }

    /// Format events processed total
    pub fn format_events_processed_total(&self) -> String {
        Self::format_i64(self.events_processed_total)
    }

    /// Format bytes processed total
    pub fn format_bytes_processed_total(&self) -> String {
        Self::format_i64(self.bytes_processed_total)
    }

    /// Format errors count
    pub fn format_errors(&self) -> String {
        Self::format_i64(self.errors)
    }
}

pub fn updater(mut state: State, mut rx: EventRx) -> StateTx {
    let (tx, _) = tokio::sync::broadcast::channel(10);

    // Local sender clone
    let sender = tx.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some((name, event_type)) = rx.recv() => {
                    if let Some(r) = state.get_mut(&name) {
                        match event_type {
                            EventType::EventsProcessedTotal(v) => {
                                r.events_processed_total = v;
                            }
                            EventType::BytesProcessedTotal(v) => {
                                r.bytes_processed_total = v;
                            }
                        }

                        // Send updated map to listeners
                        let _ = sender.send(state.clone());
                    }
                }
            }
        }
    });

    tx
}
