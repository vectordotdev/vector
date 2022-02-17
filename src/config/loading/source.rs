use super::{ComponentHint, Loader, Process};
use serde_toml_merge::merge_into_table;
use std::io::Read;
use toml::{
    map::Map,
    value::{Table, Value},
};

pub struct SourceLoader {
    table: Table,
}

impl SourceLoader {
    pub fn new() -> Self {
        Self { table: Map::new() }
    }
}

impl Process for SourceLoader {
    fn prepare<R: Read>(&self, mut input: R) -> Result<(String, Vec<String>), Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        Ok((source_string, vec![]))
    }

    fn merge(&mut self, table: Table, _hint: Option<ComponentHint>) -> Result<(), Vec<String>> {
        merge_into_table(&mut self.table, table).map_err(|e| vec![e.to_string()])
    }
}

impl Loader<Table> for SourceLoader {
    fn take(self) -> Table {
        self.table
    }
}
