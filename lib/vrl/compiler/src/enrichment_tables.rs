use crate::Value;
use dyn_clone::DynClone;
use std::collections::BTreeMap;

pub trait EnrichmentTables: DynClone {
    fn get_tables(&self) -> Vec<String>;
    fn find_table_row<'a>(
        &'a self,
        table: &str,
        criteria: BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, Value>>;
    fn add_index(&mut self, table: &str, fields: Vec<&str>);
}

dyn_clone::clone_trait_object!(EnrichmentTables);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl EnrichmentTables for EmptyEnrichmentTables {
    fn get_tables(&self) -> Vec<String> {
        Vec::new()
    }

    fn find_table_row<'a>(
        &'a self,
        _table: &str,
        _criteria: BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, Value>> {
        None
    }

    fn add_index(&mut self, _table: &str, _fields: Vec<&str>) {}
}
