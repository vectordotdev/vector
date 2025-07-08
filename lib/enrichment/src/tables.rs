//! The Enrichment `TableRegistry` manages the collection of `Table`s loaded
//! into Vector. Enrichment Tables go through two stages.
//!
//! ## 1. Writing
//!
//! The tables are loaded. There are two elements that need loading. The first
//! is the actual data. This is loaded at config load time, the actual loading
//! is performed by the implementation of the `EnrichmentTable` trait. Next, the
//! tables are passed through Vectors `Transform` components, particularly the
//! `Remap` transform. These Transforms are able to determine which fields we
//! will want to lookup whilst Vector is running. They can notify the tables of
//! these fields so that the data can be indexed.
//!
//! During this phase, the data is loaded within a single thread, so can be
//! loaded directly into a `HashMap`.
//!
//! ## 2. Reading
//!
//! Once all the data has been loaded we can move to the next stage. This is
//! signified by calling the `finish_load` method. At this point all the data is
//! swapped into the `ArcSwap` of the `tables` field. `ArcSwap` provides
//! lock-free read-only access to the data. From this point on we have fast,
//! efficient read-only access and can no longer add indexes or otherwise mutate
//! the data.
//!
//! This data within the `ArcSwap` is accessed through the `TableSearch`
//! struct. Any transform that needs access to this can call
//! `TableRegistry::as_readonly`. This returns a cheaply clonable struct that
//! implements `vrl:EnrichmentTableSearch` through with the enrichment tables
//! can be searched.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwap;
use vrl::value::ObjectMap;

use super::{Conditions, IndexHandle, Table};
use crate::Case;

/// A hashmap of name => implementation of an enrichment table.
type TableMap = HashMap<String, Box<dyn Table + Send + Sync>>;

#[derive(Clone, Default)]
pub struct TableRegistry {
    loading: Arc<Mutex<Option<TableMap>>>,
    tables: Arc<ArcSwap<Option<TableMap>>>,
}

/// Pessimistic Eq implementation for caching purposes
impl PartialEq for TableRegistry {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.tables, &other.tables) && Arc::ptr_eq(&self.loading, &other.loading)
            || self.tables.load().is_none()
                && other.tables.load().is_none()
                && self.loading.lock().expect("lock poison").is_none()
                && other.loading.lock().expect("lock poison").is_none()
    }
}
impl Eq for TableRegistry {}

impl TableRegistry {
    /// Load the given Enrichment Tables into the registry. This can be new tables
    /// loaded from the config, or tables that need to be reloaded because the
    /// underlying data has changed.
    ///
    /// If there are no tables currently loaded into the registry, this is a
    /// simple operation, we simply load the tables into the `loading` field.
    ///
    /// If there are tables that have already been loaded things get a bit more
    /// complicated. This can occur when the config is reloaded. Vector will be
    /// currently running and transforming events, thus the tables loaded into
    /// the `tables` field could be in active use. Since there is no lock
    /// against these tables, we cannot mutate this list. We do need to have a
    /// full list of tables in the `loading` field since there may be some
    /// transforms that will need to add indexes to these tables during the
    /// reload.
    ///
    /// Our only option is to clone the data that is in `tables` and move it
    /// into the `loading` field so it can be mutated. This could be a
    /// potentially expensive operation. For the period whilst the config is
    /// reloading we could potentially have double the enrichment data loaded
    /// into memory.
    ///
    /// Once loading is complete, the data is swapped out of `loading` and we
    /// return to a single copy of the tables.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    pub fn load(&self, mut tables: TableMap) {
        let mut loading = self.loading.lock().unwrap();
        let existing = self.tables.load();
        if let Some(existing) = &**existing {
            // We already have some tables
            let extend = existing
                .iter()
                .filter(|(key, _)| !tables.contains_key(*key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<HashMap<_, _>>();

            tables.extend(extend);
        }
        match *loading {
            None => *loading = Some(tables),
            Some(ref mut loading) => loading.extend(tables),
        }
    }

    /// Swap the data out of the `HashTable` into the `ArcSwap`.
    ///
    /// From this point we can no longer add indexes to the tables, but are now
    /// allowed to read the data.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    pub fn finish_load(&self) {
        let mut tables_lock = self.loading.lock().unwrap();
        let tables = tables_lock.take();
        self.tables.swap(Arc::new(tables));
    }

    /// Return a list of the available tables that we can write to.
    ///
    /// This only works in the writing stage and will acquire a lock to retrieve
    /// the tables.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    pub fn table_ids(&self) -> Vec<String> {
        let locked = self.loading.lock().unwrap();
        match *locked {
            Some(ref tables) => tables.iter().map(|(key, _)| key.clone()).collect(),
            None => Vec::new(),
        }
    }

    /// Adds an index to the given Enrichment Table.
    ///
    /// If we are in the reading stage, this function will error.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    pub fn add_index(
        &mut self,
        table: &str,
        case: Case,
        fields: &[&str],
    ) -> Result<IndexHandle, String> {
        let mut locked = self.loading.lock().unwrap();

        match *locked {
            None => Err("finish_load has been called".to_string()),
            Some(ref mut tables) => match tables.get_mut(table) {
                None => Err(format!("table '{}' not loaded", table)),
                Some(table) => table.add_index(case, fields),
            },
        }
    }

    /// Returns a cheaply clonable struct through that provides lock free read
    /// access to the enrichment tables.
    pub fn as_readonly(&self) -> TableSearch {
        TableSearch(self.tables.clone())
    }

    /// Returns the indexes that have been applied to the given table.
    /// If the table is reloaded we need these to reapply them to the new reloaded tables.
    pub fn index_fields(&self, table: &str) -> Vec<(Case, Vec<String>)> {
        match &**self.tables.load() {
            Some(tables) => tables
                .get(table)
                .map(|table| table.index_fields())
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Checks if the table needs reloading.
    /// If in doubt (the table isn't in our list) we return true.
    pub fn needs_reload(&self, table: &str) -> bool {
        match &**self.tables.load() {
            Some(tables) => tables
                .get(table)
                .map(|table| table.needs_reload())
                .unwrap_or(true),
            None => true,
        }
    }
}

impl std::fmt::Debug for TableRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_enrichment_table(f, "TableRegistry", &self.tables)
    }
}

/// Provides read only access to the enrichment tables via the
/// `vrl::EnrichmentTableSearch` trait. Cloning this object is designed to be
/// cheap. The underlying data will be shared by all clones.
#[derive(Clone, Default)]
pub struct TableSearch(Arc<ArcSwap<Option<TableMap>>>);

impl TableSearch {
    /// Search the given table to find the data.
    ///
    /// If we are in the writing stage, this function will return an error.
    pub fn find_table_row<'a>(
        &self,
        table: &str,
        case: Case,
        condition: &'a Conditions<'a>,
        select: Option<&[String]>,
        index: &[IndexHandle],
    ) -> Result<ObjectMap, String> {
        let tables = self.0.load();
        if let Some(ref tables) = **tables {
            match tables.get(table) {
                None => Err(format!("table {} not loaded", table)),
                Some(table) => table.find_table_row(case, condition, select, index),
            }
        } else {
            Err("finish_load not called".to_string())
        }
    }

    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    /// Can return multiple matched records
    pub fn find_table_rows<'a>(
        &self,
        table: &str,
        case: Case,
        condition: &'a Conditions<'a>,
        select: Option<&[String]>,
        index: &[IndexHandle],
    ) -> Result<Vec<ObjectMap>, String> {
        let tables = self.0.load();
        if let Some(ref tables) = **tables {
            match tables.get(table) {
                None => Err(format!("table {} not loaded", table)),
                Some(table) => table.find_table_rows(case, condition, select, index),
            }
        } else {
            Err("finish_load not called".to_string())
        }
    }
}

impl std::fmt::Debug for TableSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_enrichment_table(f, "EnrichmentTableSearch", &self.0)
    }
}

/// Provide some fairly rudimentary debug output for enrichment tables.
fn fmt_enrichment_table(
    f: &mut std::fmt::Formatter<'_>,
    name: &'static str,
    tables: &Arc<ArcSwap<Option<TableMap>>>,
) -> std::fmt::Result {
    let tables = tables.load();
    match **tables {
        Some(ref tables) => {
            let mut tables = tables.iter().fold(String::from("("), |mut s, (key, _)| {
                s.push_str(key);
                s.push_str(", ");
                s
            });

            tables.truncate(std::cmp::max(tables.len(), 0));
            tables.push(')');

            write!(f, "{} {}", name, tables)
        }
        None => write!(f, "{} loading", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_util::DummyEnrichmentTable, Condition};
    use vrl::value::Value;

    #[test]
    fn tables_loaded() {
        let mut tables: TableMap = HashMap::new();
        tables.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));
        tables.insert("dummy2".to_string(), Box::new(DummyEnrichmentTable::new()));

        let registry = super::TableRegistry::default();
        registry.load(tables);
        let mut result = registry.table_ids();
        result.sort();
        assert_eq!(vec!["dummy1", "dummy2"], result);
    }

    #[test]
    fn can_add_indexes() {
        let mut tables: TableMap = HashMap::new();
        let indexes = Arc::new(Mutex::new(Vec::new()));
        let dummy = DummyEnrichmentTable::new_with_index(indexes.clone());
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let mut registry = super::TableRegistry::default();
        registry.load(tables);
        assert_eq!(
            Ok(IndexHandle(0)),
            registry.add_index("dummy1", Case::Sensitive, &["erk"])
        );

        let indexes = indexes.lock().unwrap();
        assert_eq!(vec!["erk".to_string()], *indexes[0]);
    }

    #[test]
    fn can_not_find_table_row_before_finish() {
        let mut tables: TableMap = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let registry = super::TableRegistry::default();
        registry.load(tables);
        let tables = registry.as_readonly();

        assert_eq!(
            Err("finish_load not called".to_string()),
            tables.find_table_row(
                "dummy1",
                Case::Sensitive,
                &[vec![Condition::Equals {
                    field: "thing",
                    value: Value::from("thang"),
                }]],
                None,
                &[]
            )
        );
    }

    #[test]
    fn can_not_add_indexes_after_finish() {
        let mut tables: TableMap = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let mut registry = super::TableRegistry::default();
        registry.load(tables);
        registry.finish_load();
        assert_eq!(
            Err("finish_load has been called".to_string()),
            registry.add_index("dummy1", Case::Sensitive, &["erk"])
        );
    }

    #[test]
    fn can_find_table_row_after_finish() {
        let mut tables: TableMap = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));

        let registry = super::TableRegistry::default();
        registry.load(tables);
        let tables_search = registry.as_readonly();

        registry.finish_load();

        assert_eq!(
            Ok(ObjectMap::from([("field".into(), Value::from("result"))])),
            tables_search.find_table_row(
                "dummy1",
                Case::Sensitive,
                &[vec![Condition::Equals {
                    field: "thing",
                    value: Value::from("thang"),
                }]],
                None,
                &[]
            )
        );
    }

    #[test]
    fn can_reload() {
        let mut tables: TableMap = HashMap::new();
        tables.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));

        let registry = super::TableRegistry::default();
        registry.load(tables);

        assert_eq!(vec!["dummy1".to_string()], registry.table_ids());

        registry.finish_load();

        // After we finish load there are no tables in the list
        assert!(registry.table_ids().is_empty());

        let mut tables: TableMap = HashMap::new();
        tables.insert("dummy2".to_string(), Box::new(DummyEnrichmentTable::new()));

        // A load should put both tables back into the list.
        registry.load(tables);
        let mut table_ids = registry.table_ids();
        table_ids.sort();

        assert_eq!(vec!["dummy1".to_string(), "dummy2".to_string()], table_ids,);
    }

    #[test]
    fn reloads_existing_tables() {
        let mut tables: TableMap = HashMap::new();
        tables.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));
        tables.insert("dummy2".to_string(), Box::new(DummyEnrichmentTable::new()));

        let registry = super::TableRegistry::default();
        registry.load(tables);
        registry.finish_load();

        // After we finish load there are no tables in the list
        assert!(registry.table_ids().is_empty());

        let mut new_data = ObjectMap::new();
        new_data.insert("thing".into(), Value::Null);

        let mut tables: TableMap = HashMap::new();
        tables.insert(
            "dummy2".to_string(),
            Box::new(DummyEnrichmentTable::new_with_data(new_data)),
        );

        // A load should put both tables back into the list.
        registry.load(tables);
        let tables = registry.loading.lock().unwrap();
        let tables = tables.clone().unwrap();

        // dummy1 should still have old data.
        assert_eq!(
            Value::from("result"),
            tables
                .get("dummy1")
                .unwrap()
                .find_table_row(Case::Sensitive, &[], None, &[])
                .unwrap()
                .get("field")
                .cloned()
                .unwrap()
        );

        // dummy2 should have new data.
        assert_eq!(
            Value::Null,
            tables
                .get("dummy2")
                .unwrap()
                .find_table_row(Case::Sensitive, &[], None, &[])
                .unwrap()
                .get("thing")
                .cloned()
                .unwrap()
        );
    }
}
