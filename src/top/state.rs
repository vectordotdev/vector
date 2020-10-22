use num_format::{Locale, ToFormattedString};
use std::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Mutex, RwLock, RwLockReadGuard},
};
use tui::widgets::TableState;

static ACQUIRE_LOCK_INVARIANT: &'static str = "Couldn't acquire topology lock. Please report.";
pub static TOPOLOGY_HEADERS: [&'static str; 5] = ["Name", "Type", "Events", "Errors", "Throughput"];

#[derive(Debug, Clone)]
pub struct TopologyRow {
    pub name: String,
    pub topology_type: String,
    pub events_processed_total: i64,
    pub errors: i64,
    pub throughput: f64,
}

impl TopologyRow {
    pub fn format_events_processed_total(&self) -> String {
        match self.events_processed_total {
            0 => "--".into(),
            _ => self.events_processed_total.to_formatted_string(&Locale::en),
        }
    }

    pub fn format_errors(&self) -> String {
        match self.errors {
            0 => "--".into(),
            _ => self.errors.to_formatted_string(&Locale::en),
        }
    }

    pub fn format_throughput(&self) -> String {
        match self.throughput {
            0.00 => "--".into(),
            _ => self.throughput.to_string(),
        }
    }
}

pub struct TopologyState {
    state: TableState,
    rows: RwLock<BTreeMap<String, TopologyRow>>,
}

impl TopologyState {
    /// Creates new, empty topology state
    pub fn new() -> Self {
        Self {
            state: TableState::default(),
            rows: RwLock::new(BTreeMap::new()),
        }
    }

    /// Updates topology rows by merging in changes. Rows that don't exist in `rows` will be
    /// deleted; new rows will be added, and existing rows will be updated
    pub fn update_rows(&self, rows: Vec<TopologyRow>) {
        let rows = rows
            .into_iter()
            .map(|r| {
                (
                    r.name.clone(),
                    match self.rows.read().expect(ACQUIRE_LOCK_INVARIANT).get(&r.name) {
                        Some(existing) if existing.topology_type == r.topology_type => {
                            // TODO - update values > 0. For now, just return row
                            r
                        }
                        _ => r,
                    },
                )
            })
            .collect();

        *self.rows.write().expect(ACQUIRE_LOCK_INVARIANT) = rows;
    }

    pub fn rows(&self) -> RwLockReadGuard<'_, BTreeMap<String, TopologyRow>> {
        self.rows.read().expect(ACQUIRE_LOCK_INVARIANT)
    }
}

pub struct HostMetricsState {
    memory_free_bytes: f64,
    memory_available_bytes: f64,
}

impl HostMetricsState {
    /// Update memory free bytes
    pub fn update_memory_free_bytes(&mut self, val: f64) {
        self.memory_free_bytes = val;
    }

    /// Update memory available bytes
    pub fn update_memory_available_bytes(&mut self, val: f64) {
        self.memory_available_bytes = val;
    }
}
