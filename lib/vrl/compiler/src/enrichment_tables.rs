use dyn_clone::DynClone;
use std::collections::BTreeMap;

pub trait EnrichmentTableSetup {
    fn get_tables(&self) -> Vec<String>;
    fn add_index(&mut self, table: &str, fields: Vec<&str>) -> Result<(), String>;
}

pub trait EnrichmentTableSearch: DynClone {
    fn find_table_row(
        &self,
        table: &str,
        criteria: BTreeMap<String, String>,
    ) -> Result<Option<Vec<String>>, String>;
}

dyn_clone::clone_trait_object!(EnrichmentTableSearch);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl EnrichmentTableSetup for EmptyEnrichmentTables {
    fn get_tables(&self) -> Vec<String> {
        Vec::new()
    }

    fn add_index(&mut self, _table: &str, _fields: Vec<&str>) -> Result<(), String> {
        Ok(())
    }
}

impl EnrichmentTableSearch for EmptyEnrichmentTables {
    fn find_table_row(
        &self,
        _table: &str,
        _criteria: BTreeMap<String, String>,
    ) -> Result<Option<Vec<String>>, String> {
        Ok(None)
    }
}
