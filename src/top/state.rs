use std::collections::btree_map::BTreeMap;
use tokio::sync::mpsc;

pub static COMPONENT_HEADERS: [&str; 6] = ["Name", "Kind", "Type", "Events", "Bytes", "Errors"];

pub type State = BTreeMap<String, ComponentRow>;
pub type EventTx = mpsc::Sender<(String, EventType)>;
pub type EventRx = mpsc::Receiver<(String, EventType)>;
pub type StateRx = mpsc::Receiver<State>;

#[derive(Debug)]
pub enum EventType {
    EventsProcessedTotal(i64),
    BytesProcessedTotal(i64),
    ComponentAdded(ComponentRow),
    ComponentRemoved(String),
}

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub name: String,
    pub kind: String,
    pub component_type: String,
    pub events_processed_total: i64,
    pub bytes_processed_total: i64,
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
            if let Some((name, event_type)) = event_rx.recv().await {
                match event_type {
                    EventType::EventsProcessedTotal(v) => {
                        if let Some(r) = state.get_mut(&name) {
                            r.events_processed_total = v;
                        }
                    }
                    EventType::BytesProcessedTotal(v) => {
                        if let Some(r) = state.get_mut(&name) {
                            r.bytes_processed_total = v;
                        }
                    }
                    EventType::ComponentAdded(c) => {
                        let _ = state.insert(name, c);
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
