use num_format::{Locale, ToFormattedString};
use tui::widgets::TableState;

pub static TOPOLOGY_HEADERS: [&'static str; 5] = ["Name", "Type", "Events", "Errors", "Throughput"];

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
        match self.errors {
            0 => "--".into(),
            _ => self.errors.to_string(),
        }
    }
}

pub struct TopologyState {
    state: TableState,
    rows: Vec<TopologyRow>,
}

impl TopologyState {
    pub fn new(rows: Vec<TopologyRow>) -> Self {
        Self {
            state: TableState::default(),
            rows,
        }
    }

    pub fn rows(&self) -> &Vec<TopologyRow> {
        &self.rows
    }
}
