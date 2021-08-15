pub mod enrichment_tables;

pub use enrichment_tables::EnrichmentTables;

use std::collections::BTreeMap;

pub use vrl_core::IndexHandle;

pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row(
        &self,
        criteria: BTreeMap<&str, String>,
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, String>, String>;

    /// Add an index to the data. It is the callers responsibility to pass the correct IndexHandle
    /// when searching the data, for performance reasons the enrichment table will not be
    /// responsible for checking that the index matches the fields being searched.
    fn add_index(&mut self, fields: Vec<&str>) -> Result<IndexHandle, String>;
}
