use super::{recursive::merge_with_value, ComponentHint, Loader, Process};
use crate::config::loading::loader::process::ProcessedFile;
use crate::config::{
    format, EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition, TransformOuter,
};
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
        file: &ProcessedFile,
        format: Format,
        hint: Option<ComponentHint>,
    ) -> Result<(), Vec<String>> {
        let mut table = Table::new();

        // If there's a component hint, host it under a component field. Otherwise, deserialize
        // as a 'root' value.
        if let Some(hint) = hint {
            let mut component = Table::new();
            component.insert(file.name.clone(), format::deserialize(&file.input, format)?);

            table.insert(
                hint.as_component_field().to_owned(),
                Value::Table(component),
            );
        } else {
            table = format::deserialize(&file.input, format)?;
        }

        merge_into_table(&mut self.table, table).map_err(|e| vec![e.to_string()])
    }
}

impl Loader<Table> for SourceLoader {
    fn take(self) -> Table {
        self.table
    }
}
