use num_format::{Locale, ToFormattedString};
use std::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

pub static TOPOLOGY_HEADERS: [&str; 5] = ["Name", "Type", "Events", "Errors", "Throughput"];
pub static ACQUIRE_LOCK_INVARIANT: &str = "Unable to acquire topology lock. Please report this.";

#[derive(Debug, Clone)]
pub struct TopologyRow {
    pub name: String,
    pub topology_type: String,
    pub events_processed_total: i64,
    pub errors: i64,
    pub throughput: f64,
}

impl TopologyRow {
    /// Helper method for formatting an f64 value -> String
    fn format_f64(val: f64) -> String {
        if val.is_normal() {
            val.to_string()
        } else {
            "--".into()
        }
    }

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

    /// Format errors count
    pub fn format_errors(&self) -> String {
        Self::format_i64(self.errors)
    }

    /// Format throughput
    pub fn format_throughput(&self) -> String {
        Self::format_f64(self.throughput)
    }
}

pub struct TopologyState {
    rows: Mutex<BTreeMap<String, TopologyRow>>,
}

impl TopologyState {
    /// Creates new, empty topology state
    pub fn new() -> Self {
        Self {
            rows: Mutex::new(BTreeMap::new()),
        }
    }

    /// Immutable method that returns a new Arc<Self> containing updated rows. Rows that
    /// don't exist in `rows` will be deleted; new rows will be added, and existing
    /// rows will be updated
    pub fn update_rows(&self, rows: Vec<TopologyRow>) {
        let rows = rows
            .into_iter()
            .map(|r| {
                (
                    r.name.clone(),
                    match self.rows.lock().expect(ACQUIRE_LOCK_INVARIANT).get(&r.name) {
                        Some(existing) if existing.topology_type == r.topology_type => {
                            // TODO - update values > 0. For now, just return row
                            r
                        }
                        _ => r,
                    },
                )
            })
            .collect();

        *self.rows.lock().expect(ACQUIRE_LOCK_INVARIANT) = rows;
    }

    /// Returns a cloned copy of topology rows
    pub fn rows(&self) -> Vec<TopologyRow> {
        self.rows
            .lock()
            .expect(ACQUIRE_LOCK_INVARIANT)
            .values()
            .cloned()
            .collect()
    }
}

/// Contains the aggregate state required to render each widget in a dashboard
pub struct WidgetsState {
    url: url::Url,
    topology: Arc<TopologyState>,
    tx: watch::Sender<()>,
    rx: watch::Receiver<()>,
}

impl WidgetsState {
    /// Returns new widgets state
    pub fn new(url: url::Url, topology: TopologyState) -> Self {
        let (tx, rx) = watch::channel(());

        Self {
            url,
            topology: Arc::new(topology),
            tx,
            rx,
        }
    }

    /// Returns a thread-safe clone of current topology state
    pub fn topology(&self) -> Arc<TopologyState> {
        Arc::clone(&self.topology)
    }

    /// Returns a string representation of the URL
    pub fn url(&self) -> String {
        self.url.to_string()
    }

    /// Signal an update of state to a listener
    fn notify(&self) {
        let _ = self.tx.broadcast(());
    }

    /// Listen for an update signal. Used to determine whether the dashboard should redraw
    pub async fn listen(&self) -> watch::Receiver<()> {
        self.rx.clone()
    }

    /// Update topology rows
    pub fn update_topology_rows(&self, rows: Vec<TopologyRow>) {
        self.topology.update_rows(rows);
        self.notify();
    }
}
