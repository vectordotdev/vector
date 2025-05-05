use serde_toml_merge::merge_into_table;
use toml::{map::Map, value::Table};

use super::{ComponentHint, Loader, Process};

pub struct SourceLoader {
    table: Table,
}

impl SourceLoader {
    pub fn new() -> Self {
        Self { table: Map::new() }
    }
}

impl Process for SourceLoader {
    fn postprocess(&mut self, table: Table) -> Result<Table, Vec<String>> {
        Ok(table)
    }

    /// Merge values by combining with the internal TOML `Table`.
    fn merge(&mut self, table: Table, _hint: Option<ComponentHint>) -> Result<(), Vec<String>> {
        merge_into_table(&mut self.table, table).map_err(|e| vec![e.to_string()])
    }
}

impl Loader<Table> for SourceLoader {
    /// Returns the resulting TOML `Table`.
    fn take(self) -> Table {
        self.table
    }
}
