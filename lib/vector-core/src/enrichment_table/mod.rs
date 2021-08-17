pub mod enrichment_tables;

pub use enrichment_tables::EnrichmentTables;

use std::collections::BTreeMap;

/// Enrichment tables represent additional data sources that can be used to enrich the event data
/// passing through Vector.
pub trait EnrichmentTable: std::fmt::Debug {
    /// Search the enrichment table data with the given condition.
    /// All fields within the data much match (AND).
    fn find_table_row(&self, condition: BTreeMap<String, String>) -> Option<&Vec<String>>;

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    fn add_index(&mut self, fields: Vec<&str>);
}
