use std::collections::btree_map::BTreeMap;
use tokio::sync::mpsc;

type NamedMetric = (String, i64);

#[derive(Debug)]
pub enum EventType {
    ProcessedEventsTotals(Vec<NamedMetric>),
    /// Interval in ms + named metric
    ProcessedEventsThroughputs(i64, Vec<NamedMetric>),
    ProcessedBytesTotals(Vec<NamedMetric>),
    /// Interval + named metric
    ProcessedBytesThroughputs(i64, Vec<NamedMetric>),
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
    pub processed_events_total: i64,
    pub processed_events_throughput_sec: i64,
    pub processed_bytes_total: i64,
    pub processed_bytes_throughput_sec: i64,
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
                    EventType::ProcessedEventsTotals(rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.processed_events_total = v;
                            }
                        }
                    }
                    EventType::ProcessedEventsThroughputs(interval, rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.processed_events_throughput_sec =
                                    (v as f64 * (1000.0 / interval as f64)) as i64;
                            }
                        }
                    }
                    EventType::ProcessedBytesTotals(rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.processed_bytes_total = v;
                            }
                        }
                    }
                    EventType::ProcessedBytesThroughputs(interval, rows) => {
                        for (name, v) in rows {
                            if let Some(r) = state.get_mut(&name) {
                                r.processed_bytes_throughput_sec =
                                    (v as f64 * (1000.0 / interval as f64)) as i64;
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
