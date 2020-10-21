use num_format::{Locale, ToFormattedString};
use std::{
    collections::btree_map,
    sync::{Arc, Mutex},
};
use tui::widgets::TableState;

pub static TOPOLOGY_HEADERS: [&'static str; 5] = ["Name", "Type", "Events", "Errors", "Throughput"];

#[derive(Debug, Clone)]
pub struct TopologyRow {
    pub name: String,
    pub topology_type: String,
    pub events_processed: i64,
    pub errors: i64,
    pub throughput: f64,
}

impl TopologyRow {
    pub fn format_events_processed(&self) -> String {
        match self.events_processed {
            0 => "--".into(),
            _ => self.events_processed.to_formatted_string(&Locale::en),
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

    pub fn update_events_processed(&mut self, val: i64) {
        self.events_processed = val;
    }
}

pub struct TopologyState {
    state: TableState,
    rows: btree_map::BTreeMap<String, Arc<Mutex<TopologyRow>>>,
}

impl TopologyState {
    pub fn new(rows: Vec<TopologyRow>) -> Self {
        Self {
            state: TableState::default(),
            rows: rows
                .into_iter()
                .map(|r| (r.name.clone(), Arc::new(Mutex::new(r))))
                .collect(),
        }
    }

    pub fn rows(&self) -> btree_map::Values<String, Arc<Mutex<TopologyRow>>> {
        self.rows.values()
    }

    pub fn get_row(&self, name: &str) -> Option<&Arc<Mutex<TopologyRow>>> {
        self.rows.get(name)
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
