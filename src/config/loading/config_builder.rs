use super::{deserialize_value, loader, prepare_input};
use super::{ComponentHint, Process, ProcessedFile};
use crate::config::{
    format, ComponentKey, ConfigBuilder, EnrichmentTableOuter, Format, SinkOuter, SourceOuter,
    TestDefinition, TransformOuter,
};
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use std::io::Read;
use toml::value::{Table, Value};

pub struct ConfigBuilderLoader {
    builder: ConfigBuilder,
}

impl ConfigBuilderLoader {
    pub fn new() -> Self {
        Self {
            builder: ConfigBuilder::default(),
        }
    }
}

impl Process for ConfigBuilderLoader {
    fn prepare<R: Read>(&self, input: R) -> Result<(String, Vec<String>), Vec<String>> {
        prepare_input(input)
    }

    fn merge(
        &mut self,
        name: String,
        value: Value,
        hint: Option<ComponentHint>,
    ) -> Result<(), Vec<String>> {
        let component_key = ComponentKey::from(name);

        match hint {
            Some(ComponentHint::Source) => {
                self.builder
                    .sources
                    .insert(component_key, deserialize_value(value)?);
            }
            Some(ComponentHint::Sink) => {
                self.builder
                    .sinks
                    .insert(component_key, deserialize_value(value)?);
            }
            Some(ComponentHint::Transform) => {
                self.builder
                    .transforms
                    .insert(component_key, deserialize_value(value)?);
            }
            Some(ComponentHint::EnrichmentTable) => {
                self.builder
                    .enrichment_tables
                    .insert(component_key, deserialize_value(value)?);
            }
            Some(ComponentHint::Test) => self
                .builder
                .tests
                .extend(deserialize_value::<Vec<TestDefinition<String>>>(value)?),
            None => {
                self.builder.append(deserialize_value(value)?)?;
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
