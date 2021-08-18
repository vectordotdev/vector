use dyn_clone::DynClone;
use std::collections::BTreeMap;

use crate::Value;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct IndexHandle(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    Equals { field: String, value: String },
}

pub trait EnrichmentTableSetup: DynClone {
    fn table_ids(&self) -> Vec<String>;
    fn add_index(&mut self, table: &str, fields: &[&str]) -> Result<IndexHandle, String>;
}

dyn_clone::clone_trait_object!(EnrichmentTableSetup);

pub trait EnrichmentTableSearch: DynClone {
    fn find_table_row(
        &self,
        table: &str,
        criteria: Vec<Condition>,
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String>;
}

dyn_clone::clone_trait_object!(EnrichmentTableSearch);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl EnrichmentTableSetup for EmptyEnrichmentTables {
    fn table_ids(&self) -> Vec<String> {
        Vec::new()
    }

    fn add_index(&mut self, _table: &str, _fields: &[&str]) -> Result<IndexHandle, String> {
        Ok(IndexHandle(0))
    }
}

impl EnrichmentTableSearch for EmptyEnrichmentTables {
    fn find_table_row(
        &self,
        _table: &str,
        _condition: Vec<Condition>,
        _index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        Err("no data found".to_string())
    }
}
