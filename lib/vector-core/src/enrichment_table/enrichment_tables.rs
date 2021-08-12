use super::EnrichmentTable;
use arc_swap::ArcSwap;
#[cfg(feature = "vrl")]
use std::collections::BTreeMap;
use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

/// The Enrichment Tables manages the collection of `EnrichmentTable`s loaded into Vector.
/// Enrichment Tables go through two stages.
///
/// ## 1. Writing
///
/// The  tables are loaded. There are two elements that need loading. The first is the actual data.
/// This is loaded at config load time, the actual loading is performed by the implementation of
/// the `EnrichmentTable` trait. Next, the tables are passed through Vectors `Transform` components,
/// particularly the `Remap` transform. These Transforms are able to determine which fields we will
/// want to lookup whilst Vector is running. They can notify the tables of these fields no that the
/// data can be indexed.
///
/// During this phase, the data needs to be mutated and shared around a number of potential
/// different components. Also, performance is not absolutely critical at this phase. Thus, the
/// data is stored within a `Mutex` - stored in the `loading` field of this struct.
///
/// ## 2. Reading
///
/// Once all the data has been loaded we can move to the next stage. This is signified by calling
/// the `finish_load` method. At this point all the data is swapped out of the `Mutex` in the loading
/// field into the `ArcSwap` of the `tables` field. `ArcSwap` provides lock-free read-only access
/// to the data. From this point on we have fast, efficient read-only access and can no longer add
/// indexes or otherwise mutate the data .
///
/// Cloning this object is designed to be cheap. The underlying data will be shared by all clones.
///
#[derive(Clone)]
pub struct EnrichmentTables {
    loading: Arc<Mutex<Option<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>>,
    tables: Arc<ArcSwap<Option<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>>,
}

impl EnrichmentTables {
    pub fn new(tables: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>) -> Self {
        Self {
            loading: Arc::new(Mutex::new(Some(tables))),
            tables: Arc::new(ArcSwap::default()),
        }
    }

    /// Swap the data out of the `Mutex` into the `ArcSwap`.
    /// From this point we can no longer add indexes to the tables, but are now allowed to read the
    /// data.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    pub fn finish_load(&self) {
        let mut tables_lock = self.loading.lock().unwrap();
        let tables = tables_lock.take();
        self.tables.swap(Arc::new(tables));
    }
}

impl Default for EnrichmentTables {
    fn default() -> Self {
        Self::new(HashMap::new())
    }
}

impl std::fmt::Debug for EnrichmentTables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tables = self.tables.load();
        match **tables {
            Some(ref tables) => {
                let mut tables = tables.iter().fold(String::from("("), |mut s, (key, _)| {
                    s.push_str(key);
                    s.push_str(", ");
                    s
                });

                tables.truncate(std::cmp::max(tables.len(), 0));
                tables.push(')');

                write!(f, "EnrichmentTables {}", tables)
            }
            None => write!(f, "EnrichmentTables loading"),
        }
    }
}

#[cfg(feature = "vrl")]
impl vrl_core::EnrichmentTables for EnrichmentTables {
    /// Return a list of the available tables. This will work regardless of which mode we are in.
    /// If we are in the writing stage, this will acquire a lock to retrieve the tables.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned in write mode.
    fn get_tables(&self) -> Vec<String> {
        let tables = self.tables.load();
        (*tables).as_ref().as_ref().map_or_else(
            || {
                // We are still loading, so we much access the mutex to get the table list.
                let locked = self.loading.lock().unwrap();
                match *locked {
                    Some(ref tables) => tables.iter().map(|(key, _)| key.clone()).collect(),
                    None => Vec::new(),
                }
            },
            |tables| tables.iter().map(|(key, _)| key.clone()).collect(),
        )
    }

    /// Search the given table to find the data.
    /// If we are in the writing stage, this function will return an error.
    fn find_table_row<'a>(
        &'a self,
        table: &str,
        criteria: BTreeMap<&str, String>,
    ) -> Result<Option<BTreeMap<String, String>>, String> {
        let tables = self.tables.load_full();
        if let Some(ref tables) = *tables {
            match tables.get(table) {
                None => Err(format!("table {} not loaded", table)),
                Some(table) => Ok(table.find_table_row(criteria)),
            }
        } else {
            Err("finish_load not called".to_string())
        }
    }

    /// Adds an index to the given Enrichment Table.
    /// If we are in the reading stage, this function will error.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned.
    fn add_index(&mut self, table: &str, fields: Vec<&str>) -> Result<(), String> {
        let mut locked = self.loading.lock().unwrap();

        match *locked {
            None => Err("finish_load has been called".to_string()),
            Some(ref mut tables) => match tables.get_mut(table) {
                None => Err(format!("table {} not loaded", table)),
                Some(table) => {
                    table.add_index(fields);
                    Ok(())
                }
            },
        }
    }
}

#[cfg(all(feature = "vrl", test))]
mod tests {
    use super::*;
    use shared::btreemap;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use vrl_core::EnrichmentTables;

    #[derive(Debug)]
    struct DummyEnrichmentTable {
        data: BTreeMap<String, String>,
        indexes: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl DummyEnrichmentTable {
        fn new() -> Self {
            Self::new_with_index(Arc::new(Mutex::new(Vec::new())))
        }

        fn new_with_index(indexes: Arc<Mutex<Vec<Vec<String>>>>) -> Self {
            Self {
                data: btreemap! {
                    "field" => "result"
                },
                indexes,
            }
        }
    }

    impl EnrichmentTable for DummyEnrichmentTable {
        fn find_table_row(
            &self,
            _criteria: BTreeMap<&str, String>,
        ) -> Option<BTreeMap<String, String>> {
            Some(self.data.clone())
        }

        fn add_index(&mut self, fields: Vec<&str>) {
            let mut indexes = self.indexes.lock().unwrap();
            indexes.push(fields.iter().map(|s| (*s).to_string()).collect());
        }
    }

    #[test]
    fn tables_loaded() {
        let mut tables: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>> = HashMap::new();
        tables.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));
        tables.insert("dummy2".to_string(), Box::new(DummyEnrichmentTable::new()));

        let tables = super::EnrichmentTables::new(tables);
        let mut result = tables.get_tables();
        result.sort();
        assert_eq!(vec!["dummy1", "dummy2"], result);
    }

    #[test]
    fn can_add_indexes() {
        let mut tables: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>> = HashMap::new();
        let indexes = Arc::new(Mutex::new(Vec::new()));
        let dummy = DummyEnrichmentTable::new_with_index(indexes.clone());
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let mut tables = super::EnrichmentTables::new(tables);
        assert_eq!(Ok(()), tables.add_index("dummy1", vec!["erk"]));

        let indexes = indexes.lock().unwrap();
        assert_eq!(vec!["erk".to_string()], *indexes[0]);
    }

    #[test]
    fn can_not_find_table_row_before_finish() {
        let mut tables: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>> = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let tables = super::EnrichmentTables::new(tables);

        assert_eq!(
            Err("finish_load not called".to_string()),
            tables.find_table_row(
                "dummy1",
                btreemap! {
                    "thing" => "thang"
                }
            )
        );
    }

    #[test]
    fn can_not_add_indexes_after_finish() {
        let mut tables: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>> = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));
        let mut tables = super::EnrichmentTables::new(tables);
        tables.finish_load();
        assert_eq!(
            Err("finish_load has been called".to_string()),
            tables.add_index("dummy1", vec!["erk"])
        );
    }

    #[test]
    fn can_find_table_row_after_finish() {
        let mut tables: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>> = HashMap::new();
        let dummy = DummyEnrichmentTable::new();
        tables.insert("dummy1".to_string(), Box::new(dummy));

        let tables = super::EnrichmentTables::new(tables);
        tables.finish_load();

        assert_eq!(
            Ok(Some(btreemap! {
                "field" => "result"
            })),
            tables.find_table_row(
                "dummy1",
                btreemap! {
                    "thing" => "thang"
                }
            )
        );
    }

    #[test]
    /// All cloned objects should share the same underlying state.
    fn can_find_table_row_after_finish_cloned_objects() {
        let mut tables1: HashMap<String, Box<dyn EnrichmentTable + Send + Sync>> = HashMap::new();
        tables1.insert("dummy1".to_string(), Box::new(DummyEnrichmentTable::new()));

        let tables1 = super::EnrichmentTables::new(tables1);
        let tables2 = tables1.clone();

        // Call finish_load on tables1
        tables1.finish_load();

        // find_table_row now works on tables2
        assert_eq!(
            Ok(Some(btreemap! {
                "field" => "result"
            })),
            tables2.find_table_row(
                "dummy1",
                btreemap! {
                    "thing" => "thang"
                }
            )
        );
    }
}
