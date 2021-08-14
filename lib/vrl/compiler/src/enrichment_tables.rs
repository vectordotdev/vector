use dyn_clone::DynClone;
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct IndexHandle(pub usize);

pub trait EnrichmentTables: DynClone {
    fn get_tables(&self) -> Vec<String>;
    fn find_table_row(
        &self,
        table: &str,
        criteria: BTreeMap<&str, String>,
        index: Option<IndexHandle>,
    ) -> Result<Option<BTreeMap<String, String>>, String>;
    fn add_index(&mut self, table: &str, fields: Vec<&str>) -> Result<IndexHandle, String>;
}

dyn_clone::clone_trait_object!(EnrichmentTables);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl EnrichmentTables for EmptyEnrichmentTables {
    fn get_tables(&self) -> Vec<String> {
        Vec::new()
    }

    fn find_table_row(
        &self,
        _table: &str,
        _criteria: BTreeMap<&str, String>,
        _index: Option<IndexHandle>,
    ) -> Result<Option<BTreeMap<String, String>>, String> {
        Ok(None)
    }

    fn add_index(&mut self, _table: &str, _fields: Vec<&str>) -> Result<IndexHandle, String> {
        Ok(IndexHandle(0))
    }
}
