//! Enrichment `Tables` manages the collection of `Table`s loaded into Vector.
//! Enrichment Tables go through two stages.
//!
//! ## 1. Writing
//!
//! The  tables are loaded. There are two elements that need loading. The first is the actual data.
//! This is loaded at config load time, the actual loading is performed by the implementation of
//! the `EnrichmentTable` trait. Next, the tables are passed through Vectors `Transform` components,
//! particularly the `Remap` transform. These Transforms are able to determine which fields we will
//! want to lookup whilst Vector is running. They can notify the tables of these fields so that the
//! data can be indexed.
//!
//! During this phase, the data is loaded within a single thread, so can be loaded directly into a
//! `HashMap`.
//!
//! ## 2. Reading
//!
//! Once all the data has been loaded we can move to the next stage. This is signified by calling
//! the `finish_load` method. At this point all the data is swapped into the `ArcSwap` of the `tables`
//! field. `ArcSwap` provides lock-free read-only access to the data. From this point on we have fast,
//! efficient read-only access and can no longer add indexes or otherwise mutate the data.
//!
//! This data within the `ArcSwap` is accessed through the `TableSearch` struct. Any transform that
//! needs access to this can call `Tables::as_search`. This returns a cheaply clonable struct that
//! implements `vrl:EnrichmentTableSearch` through with the enrichment tables can be searched.
//!
use super::Table;
use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Tables {
    loading: Option<HashMap<String, Box<dyn Table + Send + Sync>>>,
    tables: Arc<ArcSwap<Option<HashMap<String, Box<dyn Table + Send + Sync>>>>>,
}

impl Default for Tables {
    fn default() -> Self {
        Self::new(HashMap::new())
    }
}

impl Tables {
    pub fn new(tables: HashMap<String, Box<dyn Table + Send + Sync>>) -> Self {
        Self {
            loading: Some(tables),
            tables: Arc::new(ArcSwap::default()),
        }
    }

    /// Swap the data out of the `HashTable` into the `ArcSwap`.
    /// From this point we can no longer add indexes to the tables, but are now allowed to read the
    /// data.
    pub fn finish_load(&mut self) {
        let tables = self.loading.take();
        self.tables.swap(Arc::new(tables));
    }

    /// Returns a cheaply clonable struct through that provides lock free read access to the
    /// enrichment tables.
    pub fn as_search(&self) -> TableSearch {
        TableSearch(self.tables.clone())
    }
}

impl std::fmt::Debug for Tables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_enrichment_table(f, "EnrichmentTables", &self.tables)
    }
}

#[cfg(feature = "vrl")]
impl vrl_core::EnrichmentTableSetup for Tables {
    /// Return a list of the available tables. This will work regardless of which mode we are in.
    /// If we are in the writing stage, this will acquire a lock to retrieve the tables.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned in write mode.
    fn table_ids(&self) -> Vec<String> {
        let tables = self.tables.load();
        (*tables).as_ref().as_ref().map_or_else(
            || {
                // We are still loading, so we must access the mutex to get the table list.
                match self.loading {
                    Some(ref tables) => tables.iter().map(|(key, _)| key.clone()).collect(),
                    None => Vec::new(),
                }
            },
            |tables| tables.iter().map(|(key, _)| key.clone()).collect(),
        )
    }

    /// Adds an index to the given Enrichment Table.
    /// If we are in the reading stage, this function will error.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    fn add_index(&mut self, table: &str, fields: Vec<&str>) -> Result<(), String> {
        match self.loading {
            None => Err("finish_load has been called".to_string()),
            Some(ref mut tables) => match tables.get_mut(table) {
                None => Err(format!("table {} not loaded", table)),
                Some(table) => {
                    table.add_index(&fields);
                    Ok(())
                }
            },
        }
    }
}

/// Provides read only access to the enrichment tables via the `vrl::EnrichmentTableSearch` trait.
/// Cloning this object is designed to be cheap. The underlying data will be shared by all clones.
#[derive(Clone)]
pub struct TableSearch(Arc<ArcSwap<Option<HashMap<String, Box<dyn Table + Send + Sync>>>>>);

impl vrl_core::EnrichmentTableSearch for TableSearch {
    /// Search the given table to find the data.
    /// If we are in the writing stage, this function will return an error.
    fn find_table_row(
        &self,
        table: &str,
        criteria: Vec<vrl_core::Condition>,
    ) -> Result<Option<Vec<String>>, String> {
        let tables = self.0.load();
        if let Some(ref tables) = **tables {
            match tables.get(table) {
                None => Err(format!("table {} not loaded", table)),
                Some(table) => Ok(table.find_table_row(criteria).cloned()),
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
    tables: &Arc<ArcSwap<Option<HashMap<String, Box<dyn Table + Send + Sync>>>>>,
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

#[cfg(all(feature = "vrl", test))]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use vrl_core::{EnrichmentTableSearch, EnrichmentTableSetup};

    #[derive(Debug, Clone)]
    struct DummyEnrichmentTable {
        data: Vec<String>,
        indexes: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl DummyEnrichmentTable {
        fn new() -> Self {
            Self {
                data: vec!["result".to_string()],
                indexes: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn new_with_index(indexes: Arc<Mutex<Vec<Vec<String>>>>) -> Self {
            Self {
                data: vec!["result".to_string()],
                indexes,
            }
        }
    }

    impl Table for DummyEnrichmentTable {
        fn find_table_row(&self, _criteria: Vec<vrl_core::Condition>) -> Option<&Vec<String>> {
            Some(&self.data)
        }

        fn add_index(&mut self, fields: &[&str]) {
            self.indexes
                .lock()
                .unwrap()
                .push(fields.iter().map(|s| (*s).to_string()).collect());
        }
    }

    #[test]
    fn tables_loaded() {
        let mut tables: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();
        tables.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));
        tables.insert("dummy2".to_string(), Box::new(DummyEnrichmentTable::new()));

        let tables = super::Tables::new(tables);
        let mut result = tables.table_ids();
        result.sort();
        assert_eq!(vec!["dummy1", "dummy2"], result);
    }

    #[test]
    fn can_add_indexes() {
        let mut tables: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();
        let indexes = Arc::new(Mutex::new(Vec::new()));
        let dummy = DummyEnrichmentTable::new_with_index(indexes.clone());
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let mut tables = super::Tables::new(tables);
        assert_eq!(Ok(()), tables.add_index("dummy1", vec!["erk"]));

        let indexes = indexes.lock().unwrap();
        assert_eq!(vec!["erk".to_string()], *indexes[0]);
    }

    #[test]
    fn can_not_find_table_row_before_finish() {
        let mut tables: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let tables = super::Tables::new(tables).as_search();

        assert_eq!(
            Err("finish_load not called".to_string()),
            tables.find_table_row(
                "dummy1",
                vec![vrl_core::Condition::Equals {
                    field: "thing".to_string(),
                    value: "thang".to_string(),
                }]
            )
        );
    }

    #[test]
    fn can_not_add_indexes_after_finish() {
        let mut tables: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let mut tables = super::Tables::new(tables);
        tables.finish_load();
        assert_eq!(
            Err("finish_load has been called".to_string()),
            tables.add_index("dummy1", vec!["erk"])
        );
    }

    #[test]
    fn can_find_table_row_after_finish() {
        let mut tables: HashMap<String, Box<dyn Table + Send + Sync>> = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));

        let mut tables = super::Tables::new(tables);
        let tables_search = tables.as_search();

        tables.finish_load();

        assert_eq!(
            Ok(Some(vec!["result".to_string()])),
            tables_search.find_table_row(
                "dummy1",
                vec![vrl_core::Condition::Equals {
                    field: "thing".to_string(),
                    value: "thang".to_string(),
                }]
            )
        );
    }
}
