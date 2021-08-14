pub mod enrichment_tables;

pub use enrichment_tables::EnrichmentTables;

use std::collections::BTreeMap;

pub use vrl_core::IndexHandle;

pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row(
        &self,
        criteria: BTreeMap<&str, String>,
        index: Option<IndexHandle>,
    ) -> Option<BTreeMap<String, String>>;
    fn add_index(&mut self, fields: Vec<&str>) -> Result<IndexHandle, String>;
}
