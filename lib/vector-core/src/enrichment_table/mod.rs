pub mod enrichment_tables;

pub use enrichment_tables::EnrichmentTables;

use std::collections::BTreeMap;

pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row(&self, criteria: BTreeMap<String, String>) -> Option<&Vec<String>>;
    fn add_index(&mut self, fields: Vec<&str>);
}
