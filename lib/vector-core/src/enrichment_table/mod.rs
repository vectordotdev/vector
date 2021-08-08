pub mod enrichment_table;

pub use enrichment_table::EnrichmentTable;

use arc_swap::ArcSwap;
use std::collections::HashMap;

#[cfg(feature = "vrl")]
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct EnrichmentTables {
    tables: Arc<ArcSwap<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>,
}

impl From<Arc<ArcSwap<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>>
    for EnrichmentTables
{
    fn from(tables: Arc<ArcSwap<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>) -> Self {
        Self { tables }
    }
}

impl std::fmt::Debug for EnrichmentTables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tables = self.tables.load();
        let mut tables = tables.iter().fold(String::from("("), |mut s, (key, _)| {
            s.push_str(&key);
            s.push_str(", ");
            s
        });

        tables.truncate(std::cmp::max(tables.len(), 0));
        tables.push_str(")");

        write!(f, "EnrichmentTables {}", tables)
    }
}

lazy_static::lazy_static! {
    static ref MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

#[cfg(feature = "vrl")]
impl vrl_core::EnrichmentTables for EnrichmentTables {
    fn get_tables(&self) -> Vec<String> {
        let tables = self.tables.load();
        tables.iter().map(|(key, _)| key.clone()).collect()
    }

    fn find_table_row<'a>(
        &'a self,
        table: &str,
        criteria: BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, String>> {
        let tables = self.tables.load();
        let table = tables.get(table)?;
        table.find_table_row(criteria).map(|t| t.clone())
    }

    fn add_index(&mut self, table: &str, fields: Vec<&str>) {
        // Ensure we don't have multiple threads running this code at the same time, since the
        // enrichment_tables is essentially global data, whilst we are adding the index we are
        // swapping that data out of the structure. If there were two Remaps being compiled at the
        // same time is separate threads it could result in one compilation accessing the
        // empty enrichment tables, and thus compiling incorrectly.
        let lock = MUTEX.lock().unwrap();

        let mut tables = self.tables.swap(Default::default());
        match Arc::get_mut(&mut tables).unwrap().get_mut(table) {
            None => (),
            Some(table) => table.add_index(fields),
        }
        self.tables.swap(tables);

        drop(lock);
    }
}
