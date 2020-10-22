use arc_swap::ArcSwap;
use num_format::{Locale, ToFormattedString};
use std::{collections::btree_map::BTreeMap, sync::Arc};

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
    rows: BTreeMap<String, Arc<TopologyRow>>,
}

impl TopologyState {
    /// Creates new, empty topology state
    pub fn new() -> Self {
        Self {
            state: TableState::default(),
            rows: BTreeMap::new(),
        }
    }

    /// Convenience method that calls Self::new, and returns the result as ArcSwap<Self>
    pub fn arc_new() -> ArcSwap<Self> {
        ArcSwap::from(Arc::new(Self::new()))
    }

    /// Immutable method that returns a new Arc<Self> containing updated rows. Rows that
    /// don't exist in `rows` will be deleted; new rows will be added, and existing
    /// rows will be updated
    pub fn with_swapped_rows(&self, rows: Vec<TopologyRow>) -> Arc<Self> {
        let rows = rows
            .into_iter()
            .map(|r| {
                (
                    r.name.clone(),
                    Arc::new(match self.rows.get(&r.name) {
                        Some(existing) if existing.topology_type == r.topology_type => {
                            // TODO - update values > 0. For now, just return row
                            r
                        }
                        _ => r,
                    }),
                )
            })
            .collect();

        let mut topology = Self::new();
        topology.rows = rows;

        Arc::new(topology)
    }

    /// Returns a cloned copy of topology rows
    pub fn rows(&self) -> Vec<Arc<TopologyRow>> {
        self.rows.values().map(|r| Arc::clone(r)).collect()
    }
}

/// Contains the aggregate state required to render each widget in a dashboard
pub struct WidgetsState {
    url: url::Url,
    topology: ArcSwap<TopologyState>,
}

impl WidgetsState {
    /// Returns new widgets state
    pub fn new(url: url::Url, topology: ArcSwap<TopologyState>) -> Self {
        Self { url, topology }
    }

    /// Returns a thread-safe clone of current topology state
    pub fn topology(&self) -> ArcSwap<TopologyState> {
        ArcSwap::clone(&self.topology)
    }

    /// Returns a string representation of the URL
    pub fn url(&self) -> String {
        self.url.to_string()
    }
}
