pub mod tables;

pub use tables::{TableRegistry, TableSearch};
pub use vrl_core::Condition;

/// Enrichment tables represent additional data sources that can be used to enrich the event data
/// passing through Vector.
pub trait Table {
    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    fn find_table_row(&self, condition: Vec<Condition>) -> Option<&Vec<String>>;

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    fn add_index(&mut self, fields: &[&str]);
}
