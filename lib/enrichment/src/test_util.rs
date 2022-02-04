use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use vector_common::btreemap;
use vrl_core::Value;

use crate::{Case, Condition, IndexHandle, Table, TableRegistry};

#[derive(Debug, Clone)]
pub(crate) struct DummyEnrichmentTable {
    data: BTreeMap<String, Value>,
    indexes: Arc<Mutex<Vec<Vec<String>>>>,
}

impl DummyEnrichmentTable {
    pub(crate) fn new() -> Self {
        Self::new_with_index(Arc::new(Mutex::new(Vec::new())))
    }

    pub(crate) fn new_with_index(indexes: Arc<Mutex<Vec<Vec<String>>>>) -> Self {
        Self {
            data: btreemap! {
                "field".to_string() => Value::from("result"),
            },
            indexes,
        }
    }

    pub(crate) fn new_with_data(data: BTreeMap<String, Value>) -> Self {
        Self {
            data,
            indexes: Default::default(),
        }
    }
}

impl Table for DummyEnrichmentTable {
    fn find_table_row(
        &self,
        _case: Case,
        _condition: &[Condition],
        _select: Option<&[String]>,
        _index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        Ok(self.data.clone())
    }

    fn find_table_rows(
        &self,
        _case: Case,
        _condition: &[Condition],
        _select: Option<&[String]>,
        _index: Option<IndexHandle>,
    ) -> Result<Vec<BTreeMap<String, Value>>, String> {
        Ok(vec![self.data.clone()])
    }

    fn add_index(&mut self, _case: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        let mut indexes = self.indexes.lock().unwrap();
        indexes.push(fields.iter().map(|s| (*s).to_string()).collect());
        Ok(IndexHandle(indexes.len() - 1))
    }

    fn index_fields(&self) -> Vec<(Case, Vec<String>)> {
        Vec::new()
    }

    fn needs_reload(&self) -> bool {
        false
    }
}

/// Create a table registry with dummy data
pub(crate) fn get_table_registry() -> TableRegistry {
    let registry = TableRegistry::default();

    let mut tables: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();
    tables.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));
    tables.insert("dummy2".to_string(), Box::new(DummyEnrichmentTable::new()));

    registry.load(tables);

    registry
}

/// Create a table registry with dummy data
pub(crate) fn get_table_registry_with_tables(
    tables: Vec<(String, DummyEnrichmentTable)>,
) -> TableRegistry {
    let registry = TableRegistry::default();

    let mut tablesmap: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();

    for (name, table) in tables.into_iter() {
        tablesmap.insert(name, Box::new(table) as Box<dyn Table + Send + Sync>);
    }

    registry.load(tablesmap);

    registry
}
