use std::collections::BTreeMap;

use tokio::sync::mpsc;

use crate::config::ComponentKey;

type IdentifiedMetric = (ComponentKey, i64);

#[derive(Debug)]
pub enum EventType {
    ReceivedEventsTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    ReceivedEventsThroughputs(i64, Vec<IdentifiedMetric>),
    SentEventsTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    SentEventsThroughputs(i64, Vec<IdentifiedMetric>),
    ProcessedBytesTotals(Vec<IdentifiedMetric>),
    /// Interval + identified metric
    ProcessedBytesThroughputs(i64, Vec<IdentifiedMetric>),
    ComponentAdded(ComponentRow),
    ComponentRemoved(ComponentKey),
}

pub type State = BTreeMap<ComponentKey, ComponentRow>;
pub type EventTx = mpsc::Sender<EventType>;
pub type EventRx = mpsc::Receiver<EventType>;
pub type StateRx = mpsc::Receiver<State>;

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub key: ComponentKey,
    pub kind: String,
    pub component_type: String,
    pub processed_bytes_total: i64,
    pub processed_bytes_throughput_sec: i64,
    pub received_events_total: i64,
    pub received_events_throughput_sec: i64,
    pub sent_events_total: i64,
    pub sent_events_throughput_sec: i64,
    pub errors: i64,
}

/// Takes the receiver `EventRx` channel, and returns a `StateTx` state transmitter. This
/// represents the single destination for handling subscriptions and returning 'immutable' state
/// for re-rendering the dashboard. This approach uses channels vs. mutexes.
pub async fn updater(mut state: State, mut event_rx: EventRx) -> StateRx {
    let (tx, rx) = mpsc::channel(20);

    // Prime the receiver with the initial state
    let _ = tx.send(state.clone()).await;

    tokio::spawn(async move {
        while let Some(event_type) = event_rx.recv().await {
            match event_type {
                EventType::ReceivedEventsTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.get_mut(&key) {
                            r.received_events_total = v;
                        }
                    }
                }
                EventType::ReceivedEventsThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.get_mut(&key) {
                            r.received_events_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::SentEventsTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.get_mut(&key) {
                            r.sent_events_total = v;
                        }
                    }
                }
                EventType::SentEventsThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.get_mut(&key) {
                            r.sent_events_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::ProcessedBytesTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.get_mut(&key) {
                            r.processed_bytes_total = v;
                        }
                    }
                }
                EventType::ProcessedBytesThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.get_mut(&key) {
                            r.processed_bytes_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::ComponentAdded(c) => {
                    let _ = state.insert(c.key.clone(), c);
                }
                EventType::ComponentRemoved(key) => {
                    let _ = state.remove(&key);
                }
            }

            // Send updated map to listeners
            let _ = tx.send(state.clone()).await;
        }
    });

    rx
}
