use std::{collections::HashMap, io::Read};

use indexmap::IndexMap;
use toml::value::Table;

use super::{deserialize_table, loader, prepare_input, secret};
use super::{ComponentHint, Process};
use crate::config::{
    ComponentKey, ConfigBuilder, EnrichmentTableOuter, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};

pub struct ConfigBuilderLoader {
    builder: ConfigBuilder,
    secrets: Option<HashMap<String, String>>,
}

impl ConfigBuilderLoader {
    pub fn new() -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: None,
        }
    }

    pub fn with_secrets(secrets: HashMap<String, String>) -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: Some(secrets),
        }
    }
}

impl Process for ConfigBuilderLoader {
    /// Prepares input for a `ConfigBuilder` by interpolating environment variables.
    fn prepare<R: Read>(&mut self, input: R) -> Result<(String, Vec<String>), Vec<String>> {
        let (prepared_input, warnings) = prepare_input(input)?;
        let prepared_input = self
            .secrets
            .as_ref()
            .map(|s| secret::interpolate(&prepared_input, s))
            .unwrap_or(Ok(prepared_input))?;
        Ok((prepared_input, warnings))
    }

    /// Merge a TOML `Table` with a `ConfigBuilder`. Component types extend specific keys.
    fn merge(&mut self, table: Table, hint: Option<ComponentHint>) -> Result<(), Vec<String>> {
        match hint {
            Some(ComponentHint::Source) => {
                self.builder.sources.extend(deserialize_table::<
                    IndexMap<ComponentKey, SourceOuter>,
                >(table)?);
            }
            Some(ComponentHint::Sink) => {
                self.builder.sinks.extend(
                    deserialize_table::<IndexMap<ComponentKey, SinkOuter<_>>>(table)?,
                );
            }
            Some(ComponentHint::Transform) => {
                self.builder.transforms.extend(deserialize_table::<
                    IndexMap<ComponentKey, TransformOuter<_>>,
                >(table)?);
            }
            Some(ComponentHint::EnrichmentTable) => {
                self.builder.enrichment_tables.extend(deserialize_table::<
                    IndexMap<ComponentKey, EnrichmentTableOuter>,
                >(table)?);
            }
            Some(ComponentHint::Test) => {
                // This serializes to a `Vec<TestDefinition<_>>`, so we need to first expand
                // it to an ordered map, and then pull out the value, ignoring the keys.
                self.builder.tests.extend(
                    deserialize_table::<IndexMap<String, TestDefinition<String>>>(table)?
                        .into_iter()
                        .map(|(_, test)| test),
                );
            }
            None => {
                self.builder.append(deserialize_table(table)?)?;
            }
        };

        Ok(())
    }
}

impl loader::Loader<ConfigBuilder> for ConfigBuilderLoader {
    /// Returns the resulting `ConfigBuilder`.
    fn take(self) -> ConfigBuilder {
        self.builder
    }
}
