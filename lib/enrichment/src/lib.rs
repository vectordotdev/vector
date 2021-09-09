pub mod get_enrichment_table_record;
pub mod tables;

#[cfg(test)]
mod test_util;

use std::collections::BTreeMap;

use dyn_clone::DynClone;

pub use tables::{TableRegistry, TableSearch};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct IndexHandle(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition<'a> {
    Equals { field: &'a str, value: String },
}

/// Enrichment tables represent additional data sources that can be used to enrich the event data
/// passing through Vector.
pub trait Table: DynClone {
    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    ///
    /// # Errors
    /// Errors if no rows, or more than 1 row is found.
    fn find_table_row<'a>(
        &self,
        condition: &'a [Condition<'a>],
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, String>, String>;

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    ///
    /// # Errors
    /// Errors if the fields are not in the table.
    fn add_index(&mut self, fields: &[&str]) -> Result<IndexHandle, String>;
}

dyn_clone::clone_trait_object!(Table);

pub fn vrl_functions() -> Vec<Box<dyn vrl_core::Function>> {
    vec![
        Box::new(get_enrichment_table_record::GetEnrichmentTableRecord)
            as Box<dyn vrl_core::Function>,
    ]
}
