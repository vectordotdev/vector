use super::{recursive::merge_with_value, ComponentHint, Loader, Process};
use crate::config::{
    format, EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition, TransformOuter,
};
use serde_toml_merge::merge_into_table;
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
    fn load<R: std::io::Read>(
        &mut self,
        name: String,
        mut input: R,
        format: Format,
        hint: Option<ComponentHint>,
    ) -> Result<Vec<String>, Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        let mut table = Table::new();

        // If there's a component hint, host it under a component field. Otherwise, deserialize
        // as a 'root' value.
        if let Some(hint) = hint {
            let mut component = Table::new();
            component.insert(name, format::deserialize(&source_string, format)?);

            table.insert(
                hint.as_component_field().to_owned(),
                Value::Table(component),
            );
        } else {
            table = format::deserialize(&source_string, format)?;
        }

        merge_into_table(&mut self.table, table)
            .map_err(|e| vec![e.to_string()])
            .map(|_| vec![])
    }
}

impl Loader<Table> for SourceLoader {
    fn take(self) -> Table {
        self.table
    }
}
