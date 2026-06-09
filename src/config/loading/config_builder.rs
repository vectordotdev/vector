use std::collections::HashMap;

use indexmap::IndexMap;
use toml::value::Table;

use super::{
    ComponentHint, Process, deserialize_table, deserialize_table_wrapped,
    interpolate_toml_table_with_secrets, loader,
};
use crate::config::{
    ComponentKey, ConfigBuilder, EnrichmentTableOuter, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};

#[derive(Debug)]
pub struct ConfigBuilderLoader {
    builder: ConfigBuilder,
    secrets: Option<HashMap<String, String>>,
    interpolate_env: bool,
}

impl ConfigBuilderLoader {
    pub fn new() -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: None,
            interpolate_env: true,
        }
    }

    pub fn with_secrets(secrets: HashMap<String, String>) -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: Some(secrets),
            interpolate_env: true,
        }
    }

    pub const fn interpolate_env(mut self, interpolate: bool) -> Self {
        self.interpolate_env = interpolate;
        self
    }

    #[allow(dead_code)]
    pub const fn allow_empty(mut self, allow_empty: bool) -> Self {
        self.builder.allow_empty = allow_empty;
        self
    }
}

impl Process for ConfigBuilderLoader {
    fn should_interpolate_env(&self) -> bool {
        self.interpolate_env
    }

    fn postprocess(&mut self, table: Table) -> Result<Table, Vec<String>> {
        self.secrets
            .as_ref()
            .map(|secrets_map| interpolate_toml_table_with_secrets(&table, secrets_map))
            .unwrap_or(Ok(table))
    }

    fn merge(&mut self, table: Table, hint: Option<ComponentHint>) -> Result<(), Vec<String>> {
        match hint {
            Some(ComponentHint::Source) => {
                self.builder.sources.extend(deserialize_table_wrapped::<
                    IndexMap<ComponentKey, SourceOuter>,
                >(table, "sources")?);
            }
            Some(ComponentHint::Sink) => {
                self.builder.sinks.extend(deserialize_table_wrapped::<
                    IndexMap<ComponentKey, SinkOuter<_>>,
                >(table, "sinks")?);
            }
            Some(ComponentHint::Transform) => {
                self.builder.transforms.extend(deserialize_table_wrapped::<
                    IndexMap<ComponentKey, TransformOuter<_>>,
                >(table, "transforms")?);
            }
            Some(ComponentHint::EnrichmentTable) => {
                self.builder
                    .enrichment_tables
                    .extend(deserialize_table_wrapped::<
                        IndexMap<ComponentKey, EnrichmentTableOuter<_>>,
                    >(table, "enrichment_tables")?);
            }
            Some(ComponentHint::Test) => {
                // Tests are loaded as a name -> TestDefinition map from
                // namespaced dirs and converted to Vec<TestDefinition> at the
                // builder; the schema represents tests as a Vec, so this branch
                // skips the wrap-and-coerce path that the component hints use.
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
    fn take(self) -> ConfigBuilder {
        self.builder
    }
}
