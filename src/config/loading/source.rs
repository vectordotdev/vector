use super::{ComponentHint, Loader, Process, ProcessedFile};
use crate::config::{format, Format};
use serde_toml_merge::{merge_into_table, merge_tables};
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

    fn merge(
        &mut self,
        name: String,
        value: Value,
        hint: Option<ComponentHint>,
    ) -> Result<(), Vec<String>> {
        let mut out_table = Table::new();

        // If there's a component hint, host it under a component field. Otherwise, deserialize
        // as a 'root' value.
        if let Some(hint) = hint {
            let mut component = Table::new();
            component.insert(name, value);

            out_table.insert(
                hint.as_component_field().to_owned(),
                Value::Table(component),
            );
        } else {
            match value {
                Value::Table(table) => {
                    out_table = table;
                }
                _ => return Err(vec!["expected TOML table object".to_owned()]),
            }
        }

        merge_into_table(&mut self.table, out_table).map_err(|e| vec![e.to_string()])
    }
}

impl Loader<Table> for SourceLoader {
    fn take(self) -> Table {
        self.table
    }
}
