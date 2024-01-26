use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Local};
use ratatui::{
    style::{Color, Style},
    text::Span,
};
use tokio::sync::mpsc;
use vector_lib::internal_event::DEFAULT_OUTPUT;

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
    InitializeState(State),
    ReceivedBytesTotals(Vec<IdentifiedMetric>),
    /// Interval + identified metric
    ReceivedBytesThroughputs(i64, Vec<IdentifiedMetric>),
    ReceivedEventsTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    ReceivedEventsThroughputs(i64, Vec<IdentifiedMetric>),
    SentBytesTotals(Vec<IdentifiedMetric>),
    /// Interval + identified metric
    SentBytesThroughputs(i64, Vec<IdentifiedMetric>),
    // Identified overall metric + output-specific metrics
    SentEventsTotals(Vec<SentEventsMetric>),
    /// Interval in ms + identified overall metric + output-specific metrics
    SentEventsThroughputs(i64, Vec<SentEventsMetric>),
    ErrorsTotals(Vec<IdentifiedMetric>),
    #[cfg(feature = "allocation-tracing")]
    AllocatedBytes(Vec<IdentifiedMetric>),
    ComponentAdded(ComponentRow),
    ComponentRemoved(ComponentKey),
    ConnectionUpdated(ConnectionStatus),
}

#[derive(Debug, Copy, Clone)]
pub enum ConnectionStatus {
    // Initial state
    Pending,
    // Underlying web socket connection has dropped. Includes the delay between
    // reconnect attempts
    Disconnected(u64),
    // Connection is working
    Connected(DateTime<Local>),
}

impl ConnectionStatus {
    pub fn as_ui_spans(&self) -> Vec<Span> {
        match self {
            Self::Pending => vec![Span::styled(
                "Connecting...",
                Style::default().fg(Color::Yellow),
            )],
            Self::Disconnected(delay) => vec![
                Span::styled("Disconnected", Style::default().fg(Color::Red)),
                Span::from(format!(" (reconnecting every {} seconds)", delay / 1000)),
            ],
            Self::Connected(since) => vec![
                Span::styled("Connected", Style::default().fg(Color::Green)),
                Span::from(format!(" (since {})", since.format("%F %r %Z"))),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub connection_status: ConnectionStatus,
    pub components: BTreeMap<ComponentKey, ComponentRow>,
}

impl State {
    pub fn new(components: BTreeMap<ComponentKey, ComponentRow>) -> Self {
        Self {
            connection_status: ConnectionStatus::Pending,
            components,
        }
    }
}
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
    pub received_bytes_total: i64,
    pub received_bytes_throughput_sec: i64,
    pub received_events_total: i64,
    pub received_events_throughput_sec: i64,
    pub sent_bytes_total: i64,
    pub sent_bytes_throughput_sec: i64,
    pub sent_events_total: i64,
    pub sent_events_throughput_sec: i64,
    #[cfg(feature = "allocation-tracing")]
    pub allocated_bytes: i64,
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

/// Takes the receiver `EventRx` channel, and returns a `StateRx` state receiver. This
/// represents the single destination for handling subscriptions and returning 'immutable' state
/// for re-rendering the dashboard. This approach uses channels vs. mutexes.
pub async fn updater(mut event_rx: EventRx) -> StateRx {
    let (tx, rx) = mpsc::channel(20);

    let mut state = State::new(BTreeMap::new());
    tokio::spawn(async move {
        while let Some(event_type) = event_rx.recv().await {
            match event_type {
                EventType::InitializeState(new_state) => {
                    state = new_state;
                }
                EventType::ReceivedBytesTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_bytes_total = v;
                        }
                    }
                }
                EventType::ReceivedBytesThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_bytes_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::ReceivedEventsTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_events_total = v;
                        }
                    }
                }
                EventType::ReceivedEventsThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.received_events_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::SentBytesTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.sent_bytes_total = v;
                        }
                    }
                }
                EventType::SentBytesThroughputs(interval, rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.sent_bytes_throughput_sec =
                                (v as f64 * (1000.0 / interval as f64)) as i64;
                        }
                    }
                }
                EventType::SentEventsTotals(rows) => {
                    for m in rows {
                        if let Some(r) = state.components.get_mut(&m.key) {
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
                        if let Some(r) = state.components.get_mut(&m.key) {
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
                EventType::ErrorsTotals(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.errors = v;
                        }
                    }
                }
                #[cfg(feature = "allocation-tracing")]
                EventType::AllocatedBytes(rows) => {
                    for (key, v) in rows {
                        if let Some(r) = state.components.get_mut(&key) {
                            r.allocated_bytes = v;
                        }
                    }
                }
                EventType::ComponentAdded(c) => {
                    _ = state.components.insert(c.key.clone(), c);
                }
                EventType::ComponentRemoved(key) => {
                    _ = state.components.remove(&key);
                }
                EventType::ConnectionUpdated(status) => {
                    state.connection_status = status;
                }
            }

            // Send updated map to listeners
            _ = tx.send(state.clone()).await;
        }
    });

    rx
}
