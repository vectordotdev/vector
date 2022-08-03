use std::io::Read;

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
    /// Prepares input by simply reading bytes to a string. Unlike other loaders, there's no
    /// interpolation of environment variables. This is on purpose to preserve the original config.
    fn prepare<R: Read>(&mut self, mut input: R) -> Result<(String, Vec<String>), Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        Ok((source_string, vec![]))
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
