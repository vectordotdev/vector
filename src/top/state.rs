use std::collections::{BTreeMap, HashMap};

use tokio::sync::mpsc;
use vector_core::internal_event::DEFAULT_OUTPUT;

use crate::config::ComponentKey;

type IdentifiedMetric = (ComponentKey, i64);

#[derive(Debug)]
pub struct SentEventsMetric {
    pub key: ComponentKey,
    pub total: i64,
    pub outputs: HashMap<String, i64>,
}

#[derive(Debug)]
pub enum EventType {
    ReceivedEventsTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    ReceivedEventsThroughputs(i64, Vec<IdentifiedMetric>),
    // Identified overall metric + output-specific metrics
    SentEventsTotals(Vec<SentEventsMetric>),
    /// Interval in ms + identified overall metric + output-specific metrics
    SentEventsThroughputs(i64, Vec<SentEventsMetric>),
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

#[derive(Debug, Clone, Default)]
pub struct OutputMetrics {
    pub sent_events_total: i64,
    pub sent_events_throughput_sec: i64,
}

impl From<i64> for OutputMetrics {
    fn from(sent_events_total: i64) -> Self {
        Self {
            sent_events_total,
            sent_events_throughput_sec: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub key: ComponentKey,
    pub kind: String,
    pub component_type: String,
    pub outputs: HashMap<String, OutputMetrics>,
    pub processed_bytes_total: i64,
    pub processed_bytes_throughput_sec: i64,
    pub received_events_total: i64,
    pub received_events_throughput_sec: i64,
    pub sent_events_total: i64,
    pub sent_events_throughput_sec: i64,
    pub errors: i64,
}

impl ComponentRow {
    /// Note, we ignore `outputs` if it only contains [`DEFAULT_OUTPUT`] to avoid
    /// redundancy with information shown in the overall component row
    pub fn has_displayable_outputs(&self) -> bool {
        self.outputs.len() > 1
            || (self.outputs.len() == 1 && !self.outputs.contains_key(DEFAULT_OUTPUT))
    }
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
                    for m in rows {
                        if let Some(r) = state.get_mut(&m.key) {
                            r.sent_events_total = m.total;
                            for (id, v) in m.outputs {
                                r.outputs
                                    .entry(id)
                                    .or_insert_with(OutputMetrics::default)
                                    .sent_events_total = v;
                            }
                        }
                    }
                }
                EventType::SentEventsThroughputs(interval, rows) => {
                    for m in rows {
                        if let Some(r) = state.get_mut(&m.key) {
                            r.sent_events_throughput_sec =
                                (m.total as f64 * (1000.0 / interval as f64)) as i64;
                            for (id, v) in m.outputs {
                                let throughput = (v as f64 * (1000.0 / interval as f64)) as i64;
                                r.outputs
                                    .entry(id)
                                    .or_insert_with(OutputMetrics::default)
                                    .sent_events_throughput_sec = throughput;
                            }
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
