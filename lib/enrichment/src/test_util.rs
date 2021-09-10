use crate::{Case, Condition, IndexHandle, Table, TableRegistry};
use shared::btreemap;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};
use vrl_core::Value;

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
}

impl Table for DummyEnrichmentTable {
    fn find_table_row(
        &self,
        _case: Case,
        _condition: &[Condition],
        _index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        Ok(self.data.clone())
    }

    fn find_table_rows(
        &self,
        _case: Case,
        _condition: &[Condition],
        _index: Option<IndexHandle>,
    ) -> Result<Vec<BTreeMap<String, Value>>, String> {
        Ok(vec![self.data.clone()])
    }

    fn add_index(&mut self, _case: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        let mut indexes = self.indexes.lock().unwrap();
        indexes.push(fields.iter().map(|s| (*s).to_string()).collect());
        Ok(IndexHandle(indexes.len() - 1))
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
