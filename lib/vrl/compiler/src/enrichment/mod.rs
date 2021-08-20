use crate::Value;
use dyn_clone::DynClone;
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct IndexHandle(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition<'a> {
    Equals { field: &'a str, value: String },
}

pub trait TableSetup: DynClone {
    fn table_ids(&self) -> Vec<String>;
    fn add_index(&mut self, table: &str, fields: &[&str]) -> Result<IndexHandle, String>;
    fn as_readonly(&self) -> Box<dyn TableSearch + Send + Sync>;
}

dyn_clone::clone_trait_object!(TableSetup);

pub trait TableSearch: DynClone + std::fmt::Debug {
    fn find_table_row<'a>(
        &'a self,
        table: &str,
        condition: &'a [Condition<'a>],
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String>;
}

dyn_clone::clone_trait_object!(TableSearch);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl TableSetup for EmptyEnrichmentTables {
    fn table_ids(&self) -> Vec<String> {
        Vec::new()
    }

    fn add_index(&mut self, _table: &str, _fields: &[&str]) -> Result<IndexHandle, String> {
        Ok(IndexHandle(0))
    }

    fn as_readonly(&self) -> Box<dyn TableSearch + Send + Sync> {
        Box::new(self.clone())
    }
}

impl TableSearch for EmptyEnrichmentTables {
    fn find_table_row<'a>(
        &self,
        _table: &str,
        _condition: &'a [Condition<'a>],
        _index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        Err("no data found".to_string())
    }
}
