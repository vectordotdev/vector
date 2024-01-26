use std::collections::HashMap;
use vrl::value::{ObjectMap, Value};

#[derive(Debug, Clone)]
struct TestEnrichmentTable;

impl enrichment::Table for TestEnrichmentTable {
    fn find_table_row<'a>(
        &self,
        _case: enrichment::Case,
        _condition: &'a [enrichment::Condition<'a>],
        _select: Option<&[String]>,
        _index: Option<enrichment::IndexHandle>,
    ) -> Result<ObjectMap, String> {
        let mut result = ObjectMap::new();
        result.insert("id".into(), Value::from(1));
        result.insert("firstname".into(), Value::from("Bob"));
        result.insert("surname".into(), Value::from("Smith"));

        Ok(result)
    }

    fn find_table_rows<'a>(
        &self,
        _case: enrichment::Case,
        _condition: &'a [enrichment::Condition<'a>],
        _select: Option<&[String]>,
        _index: Option<enrichment::IndexHandle>,
    ) -> Result<Vec<ObjectMap>, String> {
        let mut result1 = ObjectMap::new();
        result1.insert("id".into(), Value::from(1));
        result1.insert("firstname".into(), Value::from("Bob"));
        result1.insert("surname".into(), Value::from("Smith"));

        let mut result2 = ObjectMap::new();
        result2.insert("id".into(), Value::from(2));
        result2.insert("firstname".into(), Value::from("Fred"));
        result2.insert("surname".into(), Value::from("Smith"));

        Ok(vec![result1, result2])
    }

    fn add_index(
        &mut self,
        _case: enrichment::Case,
        _fields: &[&str],
    ) -> Result<enrichment::IndexHandle, String> {
        Ok(enrichment::IndexHandle(1))
    }

    fn index_fields(&self) -> Vec<(enrichment::Case, Vec<String>)> {
        Vec::new()
    }

    /// Returns true if the underlying data has changed and the table needs reloading.
    fn needs_reload(&self) -> bool {
        false
    }
}

pub(crate) fn test_enrichment_table() -> enrichment::TableRegistry {
    let registry = enrichment::TableRegistry::default();
    let mut tables: HashMap<String, Box<dyn enrichment::Table + Send + Sync>> = HashMap::new();
    tables.insert("test".into(), Box::new(TestEnrichmentTable));
    registry.load(tables);

    registry
}
