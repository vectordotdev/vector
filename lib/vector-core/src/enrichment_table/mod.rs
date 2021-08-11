pub mod enrichment_tables;

pub use enrichment_tables::EnrichmentTables;

use std::collections::BTreeMap;

pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row(
        &self,
        criteria: BTreeMap<&str, String>,
    ) -> Option<&BTreeMap<String, vrl_core::Value>>;
    fn add_index(&mut self, fields: Vec<&str>);
}
