use std::collections::{BTreeMap, HashMap};

use vrl::Value;

#[derive(Debug, Clone)]
struct TestEnrichmentTable;

impl enrichment::Table for TestEnrichmentTable {
    fn find_table_row<'a>(
        &self,
        _case: enrichment::Case,
        _condition: &'a [enrichment::Condition<'a>],
        _select: Option<&[String]>,
        _index: Option<enrichment::IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let mut result = BTreeMap::new();
        result.insert("id".to_string(), vrl::Value::from(1));
        result.insert("firstname".to_string(), vrl::Value::from("Bob"));
        result.insert("surname".to_string(), vrl::Value::from("Smith"));

        Ok(result)
    }

    fn find_table_rows<'a>(
        &self,
        _case: enrichment::Case,
        _condition: &'a [enrichment::Condition<'a>],
        _select: Option<&[String]>,
        _index: Option<enrichment::IndexHandle>,
    ) -> Result<Vec<std::collections::BTreeMap<String, vrl::Value>>, String> {
        let mut result1 = BTreeMap::new();
        result1.insert("id".to_string(), vrl::Value::from(1));
        result1.insert("firstname".to_string(), vrl::Value::from("Bob"));
        result1.insert("surname".to_string(), vrl::Value::from("Smith"));

        let mut result2 = BTreeMap::new();
        result2.insert("id".to_string(), vrl::Value::from(2));
        result2.insert("firstname".to_string(), vrl::Value::from("Fred"));
        result2.insert("surname".to_string(), vrl::Value::from("Smith"));

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
    tables.insert("test".to_string(), Box::new(TestEnrichmentTable));
    registry.load(tables);

    registry
}
