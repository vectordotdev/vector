use std::collections::btree_map::BTreeMap;
use tokio::sync::mpsc;

pub static COMPONENT_HEADERS: [&str; 8] = [
    "Name", "Kind", "Type", "Events", "I/O", "Bytes", "I/O", "Errors",
];

type NamedMetric = (String, i64);

#[derive(Debug)]
pub enum EventType {
    EventsProcessedTotalBatch(Vec<NamedMetric>),
    EventsProcessedThroughputBatch(Vec<NamedMetric>),
    BytesProcessedTotalBatch(Vec<NamedMetric>),
    BytesProcessedThroughputBatch(Vec<NamedMetric>),
    ComponentAdded(ComponentRow),
    ComponentRemoved(String),
}

pub type State = BTreeMap<String, ComponentRow>;
pub type EventTx = mpsc::Sender<EventType>;
pub type EventRx = mpsc::Receiver<EventType>;
pub type StateRx = mpsc::Receiver<State>;

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub name: String,
    pub kind: String,
    pub component_type: String,
    pub events_processed_total: i64,
    pub events_processed_throughput: i64,
    pub bytes_processed_total: i64,
    pub bytes_processed_throughput: i64,
    pub errors: i64,
}

/// Takes the receiver `EventRx` channel, and returns a `StateTx` state transmitter. This
/// represents the single destination for handling subscriptions and returning 'immutable' state
/// for re-rendering the dashboard. This approach uses channels vs. mutexes.
pub async fn updater(mut state: State, mut event_rx: EventRx) -> StateRx {
    let (mut tx, rx) = mpsc::channel(20);

    // Prime the receiver with the initial state
    let _ = tx.send(state.clone()).await;

    tokio::spawn(async move {
        loop {
            if let Some(event_type) = event_rx.recv().await {
                match event_type {
                    EventType::EventsProcessedTotalBatch(rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.events_processed_total = v;
                            }
                        }
                    }
                    EventType::EventsProcessedThroughputBatch(rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.events_processed_throughput = v;
                            }
                        }
                    }
                    EventType::BytesProcessedTotalBatch(rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.bytes_processed_total = v;
                            }
                        }
                    }
                    EventType::BytesProcessedThroughputBatch(rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.bytes_processed_throughput = v;
                            }
                        }
                    }
                    EventType::ComponentAdded(c) => {
                        let _ = state.insert(c.name.clone(), c);
                    }
                    EventType::ComponentRemoved(name) => {
                        let _ = state.remove(&name);
                    }
                }

                // Send updated map to listeners
                let _ = tx.send(state.clone()).await;
            }
        }
    });

    rx
}
