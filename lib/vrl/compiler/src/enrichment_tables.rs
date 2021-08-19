use crate::Value;
use dyn_clone::DynClone;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub enum Condition<'a> {
    Equals { field: &'a str, value: String },
}

pub trait EnrichmentTableSetup: DynClone {
    fn table_ids(&self) -> Vec<String>;
    fn add_index(&mut self, table: &str, fields: Vec<&str>) -> Result<(), String>;
}

dyn_clone::clone_trait_object!(EnrichmentTableSetup);

pub trait EnrichmentTableSearch: DynClone {
    fn find_table_row<'a>(
        &'a self,
        table: &str,
        criteria: &'a [Condition<'a>],
    ) -> Result<Option<BTreeMap<String, Value>>, String>;
}

dyn_clone::clone_trait_object!(EnrichmentTableSearch);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl EnrichmentTableSetup for EmptyEnrichmentTables {
    fn table_ids(&self) -> Vec<String> {
        Vec::new()
    }

    fn add_index(&mut self, _table: &str, _fields: Vec<&str>) -> Result<(), String> {
        Ok(())
    }
}

impl EnrichmentTableSearch for EmptyEnrichmentTables {
    fn find_table_row<'a>(
        &self,
        _table: &str,
        _criteria: &'a [Condition<'a>],
    ) -> Result<Option<BTreeMap<String, Value>>, String> {
        Ok(None)
    }
}
