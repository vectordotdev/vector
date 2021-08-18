pub mod tables;

use std::collections::BTreeMap;

pub use tables::{TableSearch, Tables};
pub use vrl_core::{Condition, IndexHandle};

/// Enrichment tables represent additional data sources that can be used to enrich the event data
/// passing through Vector.
pub trait Table: std::fmt::Debug {
    /// Search the enrichment table data with the given condition.
    /// All fields within the data must match (AND).
    ///
    /// # Errors
    /// Errors if no rows, or more than 1 row is found.
    fn find_table_row(
        &self,
        condition: Vec<Condition>,
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, String>, String>;

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    ///
    /// # Errors
    /// Errors if the fields are not in the table.
    fn add_index(&mut self, fields: &[&str]) -> Result<IndexHandle, String>;
}
